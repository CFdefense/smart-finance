"""Tests for ChatAgent.generate_stream: the NDJSON event source behind POST /chat/stream."""

import asyncio
from collections.abc import Iterator
from typing import Any

import pytest
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, AIMessageChunk, BaseMessage, ToolCall
from langchain_core.outputs import ChatGeneration, ChatGenerationChunk, ChatResult
from langchain_core.tools import tool
from starlette.concurrency import iterate_in_threadpool

import agent.nodes.context.load as load_context_module
from agent import ChatAgent, ChatRequest, HistoryMessage
from agent.chat import build_chat_graph
from agent.logging import _run_id, _user_id

TOOL_SECRET = "TOOL-RESULT-SECRET"
SEARCH_CALL = ToolCall(name="web_search", args={"query": "emergency fund"}, id="call_1")


class StreamingFakeLLM(BaseChatModel):
    """Streams its scripted replies in pieces. A reply that is a ToolCall asks for a tool."""

    script: list[Any]
    seen: list[list[BaseMessage]] = []
    calls: int = 0

    @property
    def _llm_type(self) -> str:
        return "streaming-fake"

    def bind_tools(self, tools, **kwargs):
        """Pretend to bind tools; the graph runs them."""
        return self

    def _next(self, messages: list[BaseMessage]) -> Any:
        self.seen.append(list(messages))
        reply = self.script[self.calls]
        self.calls += 1
        return reply

    def _generate(self, messages, stop=None, run_manager=None, **kwargs) -> ChatResult:
        reply = self._next(messages)
        if isinstance(reply, tuple):
            text, tool_call = reply
            calls = [tool_call] if isinstance(tool_call, dict) else tool_call
            message = AIMessage(content=text, tool_calls=calls)
        elif isinstance(reply, dict):
            message = AIMessage(content="", tool_calls=[reply])
        else:
            message = AIMessage(content=reply)
        return ChatResult(generations=[ChatGeneration(message=message)])

    def _stream(self, messages, stop=None, run_manager=None, **kwargs) -> Iterator[Any]:
        reply = self._next(messages)
        if isinstance(reply, tuple):
            text, tool_call = reply
            calls = [tool_call] if isinstance(tool_call, dict) else tool_call
            yield ChatGenerationChunk(message=AIMessageChunk(content=text, tool_calls=calls))
            return
        if isinstance(reply, dict):
            yield ChatGenerationChunk(message=AIMessageChunk(content="", tool_calls=[reply]))
            return
        for piece in reply.split("|"):
            yield ChatGenerationChunk(message=AIMessageChunk(content=piece))


class InvokeOnlyLLM(BaseChatModel):
    """Does not implement _stream, so the graph only gets the whole reply."""

    @property
    def _llm_type(self) -> str:
        return "invoke-only"

    def bind_tools(self, tools, **kwargs):
        """Pretend to bind tools."""
        return self

    def _generate(self, messages, stop=None, run_manager=None, **kwargs) -> ChatResult:
        return ChatResult(generations=[ChatGeneration(message=AIMessage(content="whole reply"))])


class ExplodingLLM(StreamingFakeLLM):
    """Fails like an unreachable provider."""

    def _stream(self, messages, stop=None, run_manager=None, **kwargs) -> Iterator[Any]:
        raise RuntimeError("sk-secret-key rejected")
        yield  # pragma: no cover


@tool
def web_search(query: str) -> str:
    """Fake web search that returns a recognisable result."""
    return TOOL_SECRET


def make_agent(monkeypatch, llm: BaseChatModel) -> ChatAgent:
    """Build a ChatAgent around a fake model without OpenAI or a database."""
    monkeypatch.setattr(load_context_module, "load_financial_context", lambda user_id: "DATA")
    agent = ChatAgent.__new__(ChatAgent)
    agent._graph = build_chat_graph(llm, [web_search])
    return agent


def run(agent: ChatAgent, message: str = "hi", history=None) -> list[dict[str, Any]]:
    """Collect every event for one request."""
    request = ChatRequest(user_id=7, message=message, history=history or [])
    return list(agent.generate_stream(request))


def test_plain_reply_streams_tokens_then_done(monkeypatch):
    """Token events carry the reply text and the final done event holds the whole reply."""
    events = run(make_agent(monkeypatch, StreamingFakeLLM(script=["Hello|, |world"])))

    tokens = [e["text"] for e in events if e["type"] == "token"]
    assert tokens == ["Hello", ", ", "world"]
    assert events[-1] == {"type": "done", "response": "Hello, world"}
    assert [e["type"] for e in events].count("done") == 1


def test_tool_call_sends_status_before_the_answer_and_never_leaks_tool_output(monkeypatch):
    """A status event precedes the tool's result, and ToolMessage text is never a token."""
    llm = StreamingFakeLLM(script=[SEARCH_CALL, "Three|, to six months."])

    events = run(make_agent(monkeypatch, llm), "How big should my emergency fund be?")

    assert events[0] == {"type": "status", "tool": "web_search"}
    tokens = [e["text"] for e in events if e["type"] == "token"]
    assert tokens == ["Three", ", to six months."]
    assert all(TOOL_SECRET not in e.get("text", "") for e in events)
    assert TOOL_SECRET not in events[-1]["response"]
    assert events[-1] == {"type": "done", "response": "Three, to six months."}
    assert llm.calls == 2
    # The tool really ran: the second model call saw its result.
    assert any(TOOL_SECRET in str(m.content) for m in llm.seen[1])


def test_tool_call_with_preamble_tokens_accumulates_in_done_response(monkeypatch):
    """When a round streams preamble text before a tool call, all tokens are kept in done.response."""
    llm = StreamingFakeLLM(script=[("Let me check. ", SEARCH_CALL), "Done."])

    events = run(make_agent(monkeypatch, llm), "Look up my budget")

    assert any(e.get("type") == "status" and e.get("tool") == "web_search" for e in events)
    tokens = [e["text"] for e in events if e["type"] == "token"]
    assert "".join(tokens) == "Let me check. Done."
    assert events[-1] == {"type": "done", "response": "Let me check. Done."}
    assert "".join(tokens) == events[-1]["response"]


def test_history_reaches_the_model_before_the_new_message(monkeypatch):
    """Earlier turns become Human/AI messages ahead of the new message."""
    llm = StreamingFakeLLM(script=["ok"])
    history = [
        HistoryMessage(role="user", content="first question"),
        HistoryMessage(role="assistant", content="first answer"),
    ]

    run(make_agent(monkeypatch, llm), "second question", history)

    system, *rest = llm.seen[0]
    assert system.type == "system" and system.content.endswith("DATA")
    assert [(m.type, m.content) for m in rest] == [
        ("human", "first question"),
        ("ai", "first answer"),
        ("human", "second question"),
    ]


def test_graph_failure_yields_one_error_event_and_no_done(monkeypatch):
    """A crash becomes a single ai_failed error line, with no done and no exception text."""
    events = run(make_agent(monkeypatch, ExplodingLLM(script=["x"])))

    assert len(events) == 1
    assert events[0]["type"] == "error" and events[0]["code"] == "ai_failed"
    assert "sk-secret-key" not in events[0]["message"]
    assert events[0]["message"]


def test_model_that_does_not_stream_still_sends_its_text_as_a_token(monkeypatch):
    """When the model only supports invoke, the whole reply is sent as one token."""
    events = run(make_agent(monkeypatch, InvokeOnlyLLM()))

    assert events == [
        {"type": "token", "text": "whole reply"},
        {"type": "done", "response": "whole reply"},
    ]


def test_empty_reply_falls_back_to_the_gave_up_text(monkeypatch):
    """A model that answers with nothing still ends with a done event carrying a message."""
    events = run(make_agent(monkeypatch, StreamingFakeLLM(script=[""])))

    assert [e["type"] for e in events] == ["done"]
    assert "couldn't finish" in events[0]["response"]


@pytest.mark.parametrize("bad", [[{"role": "system", "content": "x"}], [{"role": "user"}]])
def test_history_rejects_bad_entries(bad):
    """Only user/assistant roles with content are valid history."""
    with pytest.raises(ValueError):
        ChatRequest(user_id=1, message="x", history=bad)


def test_run_context_survives_threadpool_steps(monkeypatch):
    """Starlette runs each step of a sync stream in its own context; the run id must persist."""
    seen: list[tuple[str, str]] = []

    class RecordingGraph:
        def stream(self, *args, **kwargs):
            for _ in range(3):
                seen.append((_run_id.get(), _user_id.get()))
                yield "updates", {"respond": {"messages": AIMessage(content="x")}}

    agent = ChatAgent.__new__(ChatAgent)
    agent._graph = RecordingGraph()
    request = ChatRequest(user_id=7, message="hi")

    async def consume() -> list[dict[str, Any]]:
        return [e async for e in iterate_in_threadpool(agent.generate_stream(request))]

    events = asyncio.run(consume())

    assert events[-1]["type"] == "done"
    assert len(seen) == 3
    assert len({run for run, _ in seen}) == 1 and seen[0][0] != "-"
    assert {user for _, user in seen} == {"7"}
