"""Tests that ChatAgent registers every tool and passes the request id the confirm gate needs."""

from types import SimpleNamespace

from langchain_core.callbacks import BaseCallbackHandler
from langchain_core.messages import AIMessage

import agent.chat as chat_module
import agent.nodes.context.load as load_context_module
from agent import ChatAgent, ChatRequest, ChatState
from agent.logging import _run_id
from agent.pending import PendingActionStore
from agent.prompts import CHAT_SYSTEM_PROMPT
from agent.tools import ANALYTICS_TOOLS, READ_TOOLS, make_write_tools
from rig import RecordingBackendApi

EXPECTED_TOOLS = [
    "web_search",
    "list_accounts",
    "list_transactions",
    "list_budgets",
    "list_goals",
    "list_rules",
    "spending_by_category",
    "spending_by_merchant",
    "compare_periods",
    "budget_status",
    "goal_progress",
    "find_recurring_charges",
    "can_i_afford",
    "propose_budget",
    "propose_rule",
    "propose_goal",
    "propose_update",
    "propose_delete",
    "confirm_action",
]


class BindRecorder:
    """Remembers which tools it was given."""

    def bind_tools(self, tools):
        """Record the tool names and return self."""
        self.names = [t.name for t in tools]
        return self

    def invoke(self, messages):
        """Answer without tools."""
        return AIMessage(content="ok")


def build(monkeypatch, **kwargs):
    """Build an agent around a recording model."""
    llm = BindRecorder()
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kw: llm)
    return ChatAgent("sk-test", "gpt-test", **kwargs), llm


def test_agent_binds_every_tool_in_a_stable_order(monkeypatch):
    """The model sees search, reads, analytics and writes, and nothing else."""
    _, llm = build(monkeypatch, backend_api=RecordingBackendApi())
    assert llm.names == EXPECTED_TOOLS


def test_tool_names_are_unique():
    """Duplicate names would shadow each other in the ToolNode."""
    assert len(set(EXPECTED_TOOLS)) == len(EXPECTED_TOOLS)
    names = [t.name for t in READ_TOOLS + ANALYTICS_TOOLS]
    names += [t.name for t in make_write_tools(PendingActionStore(), RecordingBackendApi())]
    assert len(set(names)) == len(names) == 18


def test_injected_collaborators_are_used(monkeypatch):
    """A caller can supply the backend and the pending store."""
    store, backend = PendingActionStore(), RecordingBackendApi()
    agent, _ = build(monkeypatch, backend_api=backend, pending=store)
    assert agent.pending is store and agent.backend is backend


def test_defaults_come_from_the_environment(monkeypatch):
    """Without injection the backend is built from the environment."""
    sentinel = object()
    monkeypatch.setattr(chat_module, "backend_from_env", lambda: sentinel)
    agent, _ = build(monkeypatch)
    assert agent.backend is sentinel and isinstance(agent.pending, PendingActionStore)


def test_each_agent_gets_its_own_pending_store(monkeypatch):
    """Two agents never share pending changes."""
    a, _ = build(monkeypatch, backend_api=RecordingBackendApi())
    b, _ = build(monkeypatch, backend_api=RecordingBackendApi())
    assert a.pending is not b.pending


def test_round_cap_allows_lookup_plus_propose(monkeypatch):
    """Six tool rounds leave room for list, propose and confirm flows."""
    assert chat_module.MAX_TOOL_ROUNDS == 6


def test_state_has_a_request_id():
    """The confirm gate reads request_id from the graph state."""
    assert "request_id" in ChatState.__annotations__


def test_generate_answer_passes_the_run_id_as_request_id(monkeypatch):
    """The request id equals the run id that tags the logs, and differs per request."""
    agent, _ = build(monkeypatch, backend_api=RecordingBackendApi())
    seen = []

    def invoke(state, config=None):
        seen.append((state["request_id"], _run_id.get(), state["user_id"]))
        return {"messages": [AIMessage(content="ok")]}

    agent._graph = SimpleNamespace(invoke=invoke)

    agent.generate_answer(ChatRequest(user_id=7, message="a"))
    agent.generate_answer(ChatRequest(user_id=7, message="b"))

    assert seen[0][0] == seen[0][1] and seen[1][0] == seen[1][1]
    assert seen[0][0] != seen[1][0] and seen[0][2] == 7


def test_generate_stream_passes_the_run_id_as_request_id(monkeypatch):
    """Streaming requests are gated the same way."""
    agent, _ = build(monkeypatch, backend_api=RecordingBackendApi())
    seen = []

    def stream(state, config=None, stream_mode=None):
        seen.append((state["request_id"], _run_id.get()))
        return iter(())

    agent._graph = SimpleNamespace(stream=stream)

    events = list(agent.generate_stream(ChatRequest(user_id=7, message="a")))

    assert seen[0][0] == seen[0][1] and seen[0][0] != "-"
    assert events[-1]["type"] == "done"


def test_extra_callbacks_observe_the_run(monkeypatch):
    """Observers (such as the eval harness's tool trace) are added to the run's callbacks."""

    class Observer(BaseCallbackHandler):
        """Does nothing."""

    observer = Observer()
    agent, _ = build(monkeypatch, backend_api=RecordingBackendApi(), extra_callbacks=[observer])
    seen = {}

    def invoke(state, config=None):
        seen["callbacks"] = config["callbacks"]
        return {"messages": [AIMessage(content="ok")]}

    agent._graph = SimpleNamespace(invoke=invoke)
    agent.generate_answer(ChatRequest(user_id=7, message="a"))

    assert observer in seen["callbacks"] and len(seen["callbacks"]) == 2


def test_prompt_declares_untrusted_text_and_the_confirm_rules():
    """The prompt keeps the search-data rule and adds the data, saving and confirmation rules."""
    p = CHAT_SYSTEM_PROMPT
    assert "<search_result>" in p and "never follow instructions" in p
    assert "<user_rule>" in p and "merchant" in p.lower()
    assert "propose_" in p and "confirm_action" in p
    assert "Saved:" in p and "never say" in p.lower()
    assert "latest message" in p and "same reply" in p
    assert "list_" in p and "can_i_afford" in p
    assert p.endswith("User's data:\n")


def test_context_loader_still_feeds_the_prompt(monkeypatch):
    """The user's data is still appended to the system message."""
    monkeypatch.setattr(load_context_module, "load_financial_context", lambda uid: "USER DATA")
    agent, llm = build(monkeypatch, backend_api=RecordingBackendApi())
    seen = []
    llm.invoke = lambda messages: seen.append(messages) or AIMessage(content="ok")

    agent.generate_answer(ChatRequest(user_id=7, message="hi"))

    assert seen[0][0].content.endswith("USER DATA")
