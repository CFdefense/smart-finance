import logging
from collections.abc import Iterator, Sequence
from typing import Any

from langchain_core.callbacks import BaseCallbackHandler
from langchain_core.messages import AIMessage, AIMessageChunk, HumanMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from langgraph.prebuilt import ToolNode
from pydantic import SecretStr

from agent.backend_api import BackendApi, backend_from_env
from agent.data_models import ChatRequest, ChatResponse, HistoryMessage
from agent.logging import BASE, AgentCallbackHandler, bind_run, clear_run, log_route, new_run_id
from agent.nodes import load_context, make_respond, make_scope_guard, refuse, route_after_scope
from agent.pending import PendingActionStore
from agent.scope import ScopeGuard, default_scope_guard, is_refusal
from agent.state import ChatState
from agent.tools import ANALYTICS_TOOLS, READ_TOOLS, make_write_tools, web_search

_log = logging.getLogger(f"{BASE}.chat")

MAX_TOOL_ROUNDS = 6
_UNAVAILABLE = "Sorry, the assistant is having trouble right now. Please try again shortly."
_GAVE_UP = "I couldn't finish looking that up. Please try asking again."


def route_after_respond(state: ChatState) -> str:
    """Run the tools while the model asks for them and the round cap allows; else end."""
    messages = state["messages"]
    last = messages[-1]
    if isinstance(last, AIMessage) and last.tool_calls:
        rounds = sum(1 for m in messages if isinstance(m, AIMessage) and m.tool_calls)
        if rounds > MAX_TOOL_ROUNDS:
            log_route("respond", "END", "round_cap")
            return END
        log_route("respond", "tools", f"tool_calls={len(last.tool_calls)} round={rounds}")
        return "tools"
    log_route("respond", "END", "answered")
    return END


def build_chat_graph(
    llm_with_tools, tools, guard: ScopeGuard | None = None
) -> CompiledStateGraph[Any, Any, Any, Any]:
    """
    Wire the chat workflow: [scope_guard ->] load_context -> respond <-> tools.

    A refused message goes scope_guard -> refuse -> END. `guard=None` builds the unguarded graph;
    `ChatAgent` always passes a guard.
    """
    graph = StateGraph(ChatState)
    graph.add_node("load_context", load_context)
    graph.add_node("respond", make_respond(llm_with_tools))
    graph.add_node("tools", ToolNode(tools, handle_tool_errors=True))
    if guard is None:
        graph.add_edge(START, "load_context")
    else:
        graph.add_node("scope_guard", make_scope_guard(guard))
        graph.add_node("refuse", refuse)
        graph.add_edge(START, "scope_guard")
        graph.add_conditional_edges(
            "scope_guard",
            route_after_scope,
            {"load_context": "load_context", "refuse": "refuse"},
        )
        graph.add_edge("refuse", END)
    graph.add_edge("load_context", "respond")
    graph.add_conditional_edges("respond", route_after_respond, {"tools": "tools", END: END})
    graph.add_edge("tools", "respond")
    return graph.compile()


def drop_refused_turns(history: Sequence[HistoryMessage]) -> list[HistoryMessage]:
    """Remove each user message whose reply was a fixed refusal, with that reply."""
    kept: list[HistoryMessage] = []
    for message in history:
        if message.role == "assistant" and is_refusal(message.content) and kept:
            if kept[-1].role == "user":
                kept.pop()
                continue
        kept.append(message)
    return kept


def build_messages(request: ChatRequest) -> list[Any]:
    """Convert the request's history plus its new message into graph messages."""
    messages: list[Any] = [
        HumanMessage(m.content) if m.role == "user" else AIMessage(m.content)
        for m in drop_refused_turns(request.history)
    ]
    messages.append(HumanMessage(request.message))
    return messages


def _initial_state(request: ChatRequest, run_id: str) -> dict[str, Any]:
    """Build the graph input: the messages the model sees and the raw turns the guard checks."""
    return {
        "user_id": request.user_id,
        "request_id": run_id,
        "context": "",
        "messages": build_messages(request),
        "scope_message": request.message,
        "scope_history": list(request.history),
    }


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""

    # Observers added to every run (the eval harness traces tool calls with one).
    _extra_callbacks: Sequence[BaseCallbackHandler] = ()

    def __init__(
        self,
        openai_key: str,
        model: str,
        backend_api: BackendApi | None = None,
        pending: PendingActionStore | None = None,
        extra_callbacks: Sequence[BaseCallbackHandler] | None = None,
        scope_guard: ScopeGuard | None = None,
    ):
        """Create and compile the conversational graph."""
        self.pending = pending if pending is not None else PendingActionStore()
        self.backend = backend_api if backend_api is not None else backend_from_env()
        self._extra_callbacks = tuple(extra_callbacks or ())
        tools = [
            web_search,
            *READ_TOOLS,
            *ANALYTICS_TOOLS,
            *make_write_tools(self.pending, self.backend),
        ]
        llm = ChatOpenAI(api_key=SecretStr(openai_key), model=model)
        guard = scope_guard if scope_guard is not None else default_scope_guard(openai_key)
        self._graph = build_chat_graph(llm.bind_tools(tools), tools, guard)

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Answer the user. Never raises; every log line of the run shares one run id."""
        run_id = new_run_id()
        bind_run(run_id, None, request.user_id)
        try:
            _log.info("chat.start message_chars=%d", len(request.message))
            try:
                result = self._graph.invoke(
                    _initial_state(request, run_id),
                    config={
                        "callbacks": [AgentCallbackHandler(), *self._extra_callbacks],
                        "run_name": "chat",
                    },
                )
            except Exception as exc:
                _log.error("chat.crashed type=%s", type(exc).__name__)
                _log.debug("chat.crashed detail", exc_info=True)
                return ChatResponse(response=_UNAVAILABLE)
            last = result["messages"][-1]
            answered = isinstance(last, AIMessage) and not last.tool_calls and bool(last.text)
            _log.info("chat.end answered=%s", answered)
            return ChatResponse(response=last.text if answered else _GAVE_UP)
        finally:
            clear_run()

    def generate_stream(self, request: ChatRequest) -> Iterator[dict[str, Any]]:
        """
        Answer the user as a stream of events. Never raises.

        Every log line of the run shares one run id. The run is re-bound before each step because
        a threadpool (FastAPI's, for a sync stream) gives every step its own context copy.
        """
        run_id = new_run_id()
        events = self._stream_events(request, run_id)
        try:
            while True:
                bind_run(run_id, None, request.user_id)
                try:
                    event = next(events)
                except StopIteration:
                    return
                yield event
        finally:
            events.close()
            clear_run()

    def _stream_events(self, request: ChatRequest, run_id: str) -> Iterator[dict[str, Any]]:
        """
        Answer the user as a stream of events. Never raises.

        Events: status (tool running), token (reply text), error, and a final done with the
        full reply. Only reply text from the `respond` node is streamed; tool output is not.
        The done.response text is the concatenation of all token text emitted during the stream.
        """
        _log.info(
            "chat.stream.start message_chars=%d history=%d",
            len(request.message),
            len(request.history),
        )
        emitted: list[str] = []
        streamed_round = False  # tokens already sent for the current respond round
        try:
            for mode, data in self._graph.stream(
                _initial_state(request, run_id),
                config={
                    "callbacks": [AgentCallbackHandler(), *self._extra_callbacks],
                    "run_name": "chat",
                },
                stream_mode=["messages", "updates"],
            ):
                if mode == "messages":
                    chunk, metadata = data
                    if (
                        metadata.get("langgraph_node") == "respond"
                        and isinstance(chunk, AIMessageChunk)
                        and chunk.text
                    ):
                        streamed_round = True
                        emitted.append(chunk.text)
                        yield {"type": "token", "text": chunk.text}
                    continue
                update = data.get("respond") or data.get("refuse")
                if not update:
                    continue
                message = update["messages"]
                if isinstance(message, list):
                    message = message[-1]
                if message.tool_calls:
                    for call in message.tool_calls:
                        yield {"type": "status", "tool": call["name"]}
                else:
                    if message.text and not streamed_round:  # model did not stream; send it whole
                        emitted.append(message.text)
                        yield {"type": "token", "text": message.text}
                streamed_round = False
        except Exception as exc:
            _log.error("chat.stream.crashed type=%s", type(exc).__name__)
            _log.debug("chat.stream.crashed detail", exc_info=True)
            yield {"type": "error", "code": "ai_failed", "message": _UNAVAILABLE}
            return
        final_response = "".join(emitted)
        _log.info("chat.stream.end answered=%s", bool(final_response))
        yield {"type": "done", "response": final_response or _GAVE_UP}
