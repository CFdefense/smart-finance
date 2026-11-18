"""Tests for the eval rig: the tool trace and the per-process bundle of fakes."""

from agent.backend_api import BackendError
from agent.tools import web_search
from rig import Rig, ToolTrace


def test_trace_records_the_tool_name_and_arguments(monkeypatch):
    """A real tool call through a callback config is recorded with its arguments."""
    monkeypatch.delenv("TAVILY_API_KEY", raising=False)
    trace = ToolTrace()

    web_search.invoke({"query": "roth ira limit"}, config={"callbacks": [trace]})

    assert trace.calls == [{"turn": 0, "name": "web_search", "args": {"query": "roth ira limit"}}]


def test_trace_drops_the_injected_state():
    """The graph state holds the whole conversation; it is never kept in the trace."""
    trace = ToolTrace()

    trace.on_tool_start(
        {"name": "list_transactions"},
        "ignored",
        inputs={"state": {"user_id": 7, "messages": ["secret"]}, "days": 30},
    )

    assert trace.calls == [{"turn": 0, "name": "list_transactions", "args": {"days": 30}}]


def test_trace_tags_calls_with_the_current_turn_and_resets():
    """Multi-turn evals set the turn before each message; reset goes back to turn 0."""
    trace = ToolTrace()
    trace.turn = 2
    trace.on_tool_start({"name": "confirm_action"}, "", inputs={})

    assert trace.calls[0]["turn"] == 2
    trace.reset()
    assert trace.turn == 0 and trace.calls == []


def test_trace_survives_a_callback_without_inputs():
    """Older callers pass only the string form; that must not crash a run."""
    trace = ToolTrace()

    trace.on_tool_start({"name": "list_goals"}, "{}")

    assert trace.calls == [{"turn": 0, "name": "list_goals", "args": {}}]


def test_rig_reset_clears_everything_and_sets_the_backend_mode():
    """One reset prepares a clean test: no calls, no pending actions, turn 0, chosen mode."""
    rig = Rig()
    rig.set_turn(3)
    rig.backend.send(7, "POST", "/api/budgets", {})
    rig.trace.on_tool_start({"name": "propose_budget"}, "", inputs={})
    rig.pending.add(7, "budget", {}, "preview", "r1")

    rig.reset("conflict")

    assert rig.backend.calls == [] and rig.trace.calls == [] and len(rig.pending) == 0
    assert rig.backend.mode == "conflict" and rig.backend.turn == 0 and rig.trace.turn == 0
    try:
        rig.backend.send(7, "POST", "/api/budgets", {})
    except BackendError:
        pass
    else:
        raise AssertionError("conflict mode must fail the call")


def test_set_turn_updates_the_backend_and_the_trace_together():
    """Both records must agree on which message caused a call."""
    rig = Rig()

    rig.set_turn(1)
    rig.backend.send(7, "DELETE", "/api/rules/1", None)
    rig.trace.on_tool_start({"name": "confirm_action"}, "", inputs={})

    assert rig.backend.calls[0]["turn"] == rig.trace.calls[0]["turn"] == 1
