"""End-to-end tests of the analyzer graph with scripted fake LLMs and fake data access."""

import logging
from datetime import date

import pytest
from langchain_core.messages import AIMessage

import agent.analyze as analyze_mod
import agent.nodes.analysis.claim as claim_mod
import agent.nodes.analysis.finalization as finalization_mod
import agent.nodes.analysis.load_context as load_context_mod
import agent.nodes.analysis.notification as notification_mod
import agent.tools.rules as rules_mod
from agent import AnalyzedTransaction, AnalyzeRequest, Assessment
from agent.analyze import AnalyzerAgent, build_analyzer_graph
from agent.logging import RunContextFilter
from db import Account, Rule

TXN = AnalyzedTransaction(
    transaction_id="t1",
    account_id="acc-1",
    name="Casino Royale",
    amount=250.0,
    date=date(2026, 11, 4),
    pending=False,
    category="ENTERTAINMENT",
)
REQUEST = AnalyzeRequest(user_id=7, transaction=TXN)
ASK_RULES = AIMessage(content="", tool_calls=[{"name": "get_rules", "args": {}, "id": "c1"}])


class ScriptedLLM:
    """Plays back scripted replies; the last one repeats. Raises any Exception in the script."""

    def __init__(self, *replies):
        self.replies = list(replies)
        self.calls = 0

    def invoke(self, messages):
        """Return a fresh copy of the next reply, or raise it if it is an exception."""
        self.calls += 1
        reply = self.replies.pop(0) if len(self.replies) > 1 else self.replies[0]
        if isinstance(reply, Exception):
            raise reply
        if reply is None:
            return None
        # A copy matters: LangGraph's add_messages treats a repeated message id as an update,
        # so returning the same AIMessage object twice would replace it instead of appending.
        return reply.model_copy(deep=True)


def _assessment(verdict, should_notify=False):
    return Assessment(
        verdict=verdict, summary="s", should_notify=should_notify, notify_reason="Breaks a rule"
    )


@pytest.fixture
def world(monkeypatch):
    """Fake every database and notification call; record what the graph did."""
    seen = {"sent": [], "recorded": [], "claims": []}
    monkeypatch.setattr(claim_mod, "user_owns_account", lambda u, a: True)
    monkeypatch.setattr(
        claim_mod, "claim_analysis", lambda t, u: seen["claims"].append(t) or (True, None)
    )
    monkeypatch.setattr(
        load_context_mod,
        "load_accounts",
        lambda u: [Account(account_id="acc-1", name="Checking", account_type="depository")],
    )
    monkeypatch.setattr(
        rules_mod,
        "load_rules",
        lambda u, a: [Rule(id=1, description="no gambling", account_ids=[])],
    )
    monkeypatch.setattr(
        notification_mod, "send_notification", lambda u, m: seen["sent"].append((u, m))
    )
    monkeypatch.setattr(finalization_mod, "finish_analysis", lambda *a: seen["recorded"].append(a))
    return seen


def run(investigate, assess):
    """Run the graph through a real AnalyzerAgent using the scripted LLMs."""
    agent = AnalyzerAgent.__new__(AnalyzerAgent)
    agent._graph = build_analyzer_graph(investigate, assess)
    return agent.analyze(REQUEST)


def test_alert_uses_tools_notifies_and_records(world):
    """ALERT: the model asks for rules, the rules reach assess, the user is notified."""
    investigate = ScriptedLLM(ASK_RULES, AIMessage(content="Violates gambling rule."))
    assess = ScriptedLLM(_assessment("ALERT"))

    result = run(investigate, assess)

    assert result.status == "ANALYZED" and result.notified is True
    assert result.assessment.verdict == "ALERT"
    assert world["sent"] == [(7, "Casino Royale $250.00: Breaks a rule")]
    assert world["recorded"] == [("t1", "done", "ALERT", True, None)]
    assert investigate.calls == 2


def test_ok_does_not_notify_even_if_the_model_asks(world):
    """OK never notifies, whatever the model said."""
    result = run(
        ScriptedLLM(AIMessage(content="fine")), ScriptedLLM(_assessment("OK", should_notify=True))
    )

    assert result.status == "ANALYZED" and result.notified is False
    assert world["sent"] == []


def test_review_follows_the_models_choice(world):
    """REVIEW notifies only when the model says so."""
    quiet = run(
        ScriptedLLM(AIMessage(content="hm")),
        ScriptedLLM(_assessment("REVIEW", should_notify=False)),
    )
    loud = run(
        ScriptedLLM(AIMessage(content="hm")), ScriptedLLM(_assessment("REVIEW", should_notify=True))
    )

    assert quiet.notified is False and loud.notified is True
    assert len(world["sent"]) == 1


def test_duplicate_stops_before_any_llm_call(world, monkeypatch):
    """An already-claimed transaction short-circuits: no LLM, no notification, nothing recorded."""
    monkeypatch.setattr(claim_mod, "claim_analysis", lambda t, u: (False, "done"))
    investigate, assess = ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("ALERT"))

    result = run(investigate, assess)

    assert result.status == "DUPLICATE" and result.assessment is None
    assert investigate.calls == 0 and assess.calls == 0
    assert world["sent"] == [] and world["recorded"] == []


def test_account_not_owned_ends_with_error(world, monkeypatch):
    """A foreign account is rejected before claiming and reported by error code."""
    monkeypatch.setattr(claim_mod, "user_owns_account", lambda u, a: False)

    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("OK")))

    assert result.status == "FAILED" and result.error == "account_not_owned"
    assert world["claims"] == []


def test_assess_failure_is_failed_without_notification(world):
    """If assessment fails twice nothing is sent and the claim is recorded as failed."""
    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(ValueError("a"), ValueError("b")))

    assert result.status == "FAILED" and result.error == "assess_failed"
    assert world["sent"] == []
    assert world["recorded"] == [("t1", "failed", None, False, "assess_failed")]


def test_investigate_failure_still_reaches_assess(world):
    """If the tool-using step errors, assessment still runs on the basics."""
    result = run(ScriptedLLM(RuntimeError("down")), ScriptedLLM(_assessment("OK")))

    assert result.status == "ANALYZED" and result.assessment.verdict == "OK"


def test_notify_failure_marks_the_run_failed(world, monkeypatch):
    """A delivery error is reported and the claim is recorded as failed (retryable)."""

    def boom(u, m):
        raise RuntimeError("smtp")

    monkeypatch.setattr(notification_mod, "send_notification", boom)

    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("ALERT")))

    assert result.status == "FAILED" and result.error == "notify_failed"
    assert result.notified is False
    assert world["recorded"][-1][1] == "failed"


def test_tool_loop_is_capped(world):
    """A model that asks for tools forever stops after the round cap and still assesses."""
    investigate = ScriptedLLM(ASK_RULES)  # repeats forever
    result = run(investigate, ScriptedLLM(_assessment("OK")))

    assert result.status == "ANALYZED"
    assert investigate.calls == analyze_mod.MAX_TOOL_ROUNDS + 1


def test_unexpected_graph_error_becomes_failed_result(world, monkeypatch):
    """Even a crash in a code node does not escape analyze()."""

    def boom(u, a):
        raise RuntimeError("db down")

    monkeypatch.setattr(claim_mod, "user_owns_account", boom)

    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("OK")))

    assert result.status == "FAILED" and result.error == "graph_error"


def test_no_node_ever_writes_the_transaction(world):
    """The transaction is immutable: no state update contains the `transaction` key."""
    graph = build_analyzer_graph(
        ScriptedLLM(ASK_RULES, AIMessage(content="done")), ScriptedLLM(_assessment("ALERT"))
    )
    initial = {
        "run_id": "r",
        "user_id": 7,
        "transaction": TXN,
        "context": "",
        "messages": [],
        "assessment": None,
        "notify": False,
        "notified": False,
        "status": "",
        "error": None,
    }

    updates = [u for chunk in graph.stream(initial, stream_mode="updates") for u in chunk.values()]

    assert updates and all("transaction" not in (u or {}) for u in updates)


def test_one_run_logs_a_traceable_workflow(world, caplog):
    """INFO logs show start, every node, routes, the decision and the end, under one run id."""
    caplog.handler.addFilter(RunContextFilter())
    with caplog.at_level(logging.INFO, logger="agent.analyze"):
        run(ScriptedLLM(ASK_RULES, AIMessage(content="done")), ScriptedLLM(_assessment("ALERT")))

    records = [r for r in caplog.records if r.name.startswith("agent.analyze")]
    text = "\n".join(r.getMessage() for r in records)
    for expected in (
        "analyze.start",
        "node.enter claim",
        "node.exit load_context",
        "node.enter investigate",
        "route investigate -> tools",
        "node.exit assess",
        "decision verdict=ALERT",
        "route decide -> notify",
        "node.exit notify",
        "node.exit finalize",
        "analyze.end status=ANALYZED verdict=ALERT notified=True",
    ):
        assert expected in text, expected
    assert len({r.run_ctx for r in records}) == 1
    assert next(iter({r.run_ctx for r in records})).startswith("[run=")
    assert "Casino" not in text  # user text stays out of INFO logs


def test_constructor_builds_a_working_agent(monkeypatch, world):
    """AnalyzerAgent(key, model) wires ChatOpenAI into the graph (OpenAI replaced by a fake)."""

    class FakeChat:
        """Stand-in for ChatOpenAI that returns scripted models."""

        def __init__(self, **kwargs):
            self.kwargs = kwargs

        def bind_tools(self, tools):
            """Check the four tools are bound and return a scripted investigator."""
            assert [t.name for t in tools] == [
                "get_transactions",
                "get_rules",
                "get_budgets",
                "get_goals",
            ]
            return ScriptedLLM(AIMessage(content="fine"))

        def with_structured_output(self, schema):
            """Check the Assessment schema is requested and return a scripted assessor."""
            assert schema is Assessment
            return ScriptedLLM(_assessment("OK"))

    monkeypatch.setattr(analyze_mod, "ChatOpenAI", FakeChat)

    result = AnalyzerAgent("sk-test", "gpt-test").analyze(REQUEST)

    assert result.status == "ANALYZED"


def test_context_failure_after_claim_still_records_the_run(world, monkeypatch):
    """A DB error while loading accounts must not strand the claim: the run completes."""

    def boom(u):
        raise RuntimeError("db down")

    monkeypatch.setattr(load_context_mod, "load_accounts", boom)

    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("OK")))

    assert result.status == "ANALYZED"
    assert world["recorded"] == [("t1", "done", "OK", False, None)]


def test_finalize_retries_the_write_once(world, monkeypatch):
    """A transient failure recording the outcome is retried so the claim is not left started."""
    calls = []

    def flaky(*args):
        calls.append(args)
        if len(calls) == 1:
            raise RuntimeError("blip")

    monkeypatch.setattr(finalization_mod, "finish_analysis", flaky)

    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(_assessment("OK")))

    assert result.status == "ANALYZED" and len(calls) == 2


def test_non_assessment_result_is_a_failed_attempt(world):
    """A structured-output call that returns None is retried, then fails safe."""
    result = run(ScriptedLLM(AIMessage(content="x")), ScriptedLLM(None))

    assert result.status == "FAILED" and result.error == "assess_failed"
    assert world["recorded"] == [("t1", "failed", None, False, "assess_failed")]


def test_save_graph_image_writes_the_rendered_png(tmp_path, monkeypatch):
    """The graph image is written to the requested path (rendering itself is stubbed)."""
    calls = []

    def fake_png(self, *args, **kwargs):
        calls.append(self)
        return b"\x89PNG fake"

    monkeypatch.setattr("langgraph.graph.state.CompiledStateGraph.get_graph", lambda self: self)
    monkeypatch.setattr(
        "langgraph.graph.state.CompiledStateGraph.draw_mermaid_png", fake_png, raising=False
    )

    written = analyze_mod.save_graph_image(tmp_path / "graph.png")

    assert written == tmp_path / "graph.png" and written.read_bytes() == b"\x89PNG fake"
    assert len(calls) == 1
