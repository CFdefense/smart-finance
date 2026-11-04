import logging

from langchain_core.messages import AIMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from langgraph.prebuilt import ToolNode
from pydantic import SecretStr

from agent.data_models import ChatRequest, ChatResponse
from agent.logging import BASE, AgentCallbackHandler, bind_run, clear_run, log_route, new_run_id
from agent.nodes import load_context, make_respond
from agent.state import ChatState
from agent.tools import web_search

_log = logging.getLogger(f"{BASE}.chat")


def route_after_respond(state: ChatState) -> str:
    """Run the tools while the model asks for them; otherwise end with its answer."""
    last = state["messages"][-1]
    if isinstance(last, AIMessage) and last.tool_calls:
        log_route("respond", "tools", f"tool_calls={len(last.tool_calls)}")
        return "tools"
    log_route("respond", "END", "answered")
    return END


def build_chat_graph(llm_with_tools, tools) -> CompiledStateGraph:
    """Wire the chat workflow: load_context -> respond <-> tools."""
    graph = StateGraph(ChatState)
    graph.add_node("load_context", load_context)
    graph.add_node("respond", make_respond(llm_with_tools))
    graph.add_node("tools", ToolNode(tools, handle_tool_errors=True))
    graph.add_edge(START, "load_context")
    graph.add_edge("load_context", "respond")
    graph.add_conditional_edges("respond", route_after_respond, {"tools": "tools", END: END})
    graph.add_edge("tools", "respond")
    return graph.compile()


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""

    def __init__(self, openai_key: str, model: str):
        """Create and compile the conversational graph."""
        tools = [web_search]
        llm = ChatOpenAI(api_key=SecretStr(openai_key), model=model)
        self._graph = build_chat_graph(llm.bind_tools(tools), tools)

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Generate a conversational answer; every log line of the run shares one run id."""
        run_id = new_run_id()
        bind_run(run_id, None, request.user_id)
        try:
            _log.info("chat.start message_chars=%d", len(request.message))
            result = self._graph.invoke(
                {
                    "user_id": request.user_id,
                    "context": "",
                    "messages": [request.message],
                },
                config={"callbacks": [AgentCallbackHandler()], "run_name": "chat"},
            )
            _log.info("chat.end")
            return ChatResponse(response=result["messages"][-1].text)
        finally:
            clear_run()
