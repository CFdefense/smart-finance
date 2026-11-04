"""Tests for the analyzer agent's request, result and state models."""

import pytest
from pydantic import ValidationError

from agent import AnalysisResult, AnalyzedTransaction, AnalyzeRequest, Assessment
from agent.state import AnalyzerState

TXN = {
    "transaction_id": "t1",
    "account_id": "a1",
    "name": "Best Buy",
    "amount": 900.0,
    "date": "2026-11-04",
    "pending": True,
    "category": "SHOPPING",
}


def test_request_parses_backend_shaped_transaction():
    """The request accepts the backend's TransactionResponse fields."""
    request = AnalyzeRequest.model_validate({"user_id": 7, "transaction": TXN})

    assert request.user_id == 7
    assert request.transaction.pending is True
    assert request.transaction.date.isoformat() == "2026-11-04"


def test_transaction_is_immutable():
    """Nothing can modify a transaction after it is created."""
    transaction = AnalyzedTransaction.model_validate(TXN)

    with pytest.raises(ValidationError):
        transaction.amount = 1.0


def test_assessment_defaults_and_rejects_unknown_verdict():
    """List fields default to empty; an unknown verdict is rejected."""
    assessment = Assessment(
        verdict="OK", summary="fine", should_notify=False, notify_reason="nothing unusual"
    )

    assert assessment.rule_violations == []
    with pytest.raises(ValidationError):
        Assessment(verdict="MAYBE", summary="x", should_notify=False, notify_reason="x")


def test_result_defaults():
    """A bare result is not notified and has no assessment or error."""
    result = AnalysisResult(transaction_id="t1", status="DUPLICATE")

    assert result.assessment is None
    assert result.notified is False
    assert result.error is None


def test_analyzer_state_has_expected_keys():
    """The state carries the identifiers, transaction and workflow outputs."""
    assert {
        "run_id",
        "user_id",
        "transaction",
        "context",
        "messages",
        "assessment",
        "notify",
        "notified",
        "status",
        "error",
    } <= set(AnalyzerState.__annotations__)
