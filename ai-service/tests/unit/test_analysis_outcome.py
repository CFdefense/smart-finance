"""Tests for the decide, notify and finalize nodes."""

import logging
from datetime import date

import pytest

import agent.nodes.analysis.finalization as finalize_mod
import agent.nodes.analysis.notification as notify_mod
from agent import AnalyzedTransaction, Assessment
from agent.nodes.analysis import (
    decide,
    decide_notification,
    finalize,
    notify,
    route_after_decide,
)

TXN = AnalyzedTransaction(
    transaction_id="t1",
    account_id="acc-1",
    name="Casino",
    amount=250.0,
    date=date(2026, 11, 4),
    pending=False,
    category="ENTERTAINMENT",
)


def _assessment(verdict="REVIEW", should_notify=True):
    return Assessment(
        verdict=verdict, summary="s", should_notify=should_notify, notify_reason="Breaks a rule"
    )


def _state(**kw):
    return {"user_id": 7, "transaction": TXN, "status": "STARTED", "notified": False, **kw}


@pytest.mark.parametrize(
    ("verdict", "should_notify", "expected", "rule"),
    [
        ("OK", True, False, "ok_never_notifies"),
        ("OK", False, False, "ok_never_notifies"),
        ("ALERT", False, True, "alert_always_notifies"),
        ("ALERT", True, True, "alert_always_notifies"),
        ("REVIEW", True, True, "review_follows_model"),
        ("REVIEW", False, False, "review_follows_model"),
    ],
)
def test_decide_notification_policy(verdict, should_notify, expected, rule):
    """OK never notifies, ALERT always notifies, REVIEW follows the model."""
    assert decide_notification(verdict, should_notify) == (expected, rule)


def test_decide_node_logs_the_rule_it_applied(caplog):
    """The decision log names the verdict, the model's opinion, the outcome and the rule."""
    state = _state(assessment=_assessment("REVIEW", True))

    with caplog.at_level(logging.INFO, logger="agent.analyze.decision"):
        result = decide(state)

    assert result == {"notify": True}
    assert (
        "decision verdict=REVIEW model_should_notify=True -> notify=True rule=review_follows_model"
        in caplog.text
    )
    assert route_after_decide({**state, **result}) == "notify"
    assert route_after_decide(_state(notify=False)) == "finalize"


def test_notify_sends_templated_message_to_the_state_user(monkeypatch):
    """The recipient is the state's user id; the text is the fixed template."""
    sent = []
    monkeypatch.setattr(
        notify_mod, "send_notification", lambda user_id, msg: sent.append((user_id, msg))
    )

    result = notify(_state(assessment=_assessment(), notify=True))

    assert result == {"notified": True}
    assert sent == [(7, "Casino $250.00: Breaks a rule")]


def test_notify_failure_is_reported_not_raised(monkeypatch, caplog):
    """A sending error marks the run FAILED so the claim can be retried."""

    def boom(user_id, msg):
        raise RuntimeError("smtp down")

    monkeypatch.setattr(notify_mod, "send_notification", boom)

    with caplog.at_level(logging.ERROR, logger="agent.analyze"):
        result = notify(_state(assessment=_assessment(), notify=True))

    assert result == {"notified": False, "status": "FAILED", "error": "notify_failed"}
    assert "notify.failed" in caplog.text


def test_finalize_records_done_for_a_successful_run(monkeypatch):
    """A healthy run is recorded as done with its verdict and notified flag."""
    recorded = []
    monkeypatch.setattr(finalize_mod, "finish_analysis", lambda *a: recorded.append(a))

    result = finalize(_state(assessment=_assessment("ALERT"), notified=True))

    assert result == {"status": "ANALYZED"}
    assert recorded == [("t1", "done", "ALERT", True, None)]


def test_finalize_records_failed_for_a_failed_run(monkeypatch):
    """A failed run is recorded as failed so it can be claimed again."""
    recorded = []
    monkeypatch.setattr(finalize_mod, "finish_analysis", lambda *a: recorded.append(a))

    result = finalize(_state(status="FAILED", error="assess_failed", assessment=None))

    assert result == {"status": "FAILED"}
    assert recorded == [("t1", "failed", None, False, "assess_failed")]


def test_finalize_never_raises_when_the_database_write_fails(monkeypatch, caplog):
    """If recording fails the run is still reported, and the failure is logged."""

    def boom(*args):
        raise RuntimeError("db down")

    monkeypatch.setattr(finalize_mod, "finish_analysis", boom)

    with caplog.at_level(logging.ERROR, logger="agent.analyze"):
        result = finalize(_state(assessment=_assessment()))

    assert result == {"status": "ANALYZED"}
    assert "finalize.record_failed" in caplog.text
