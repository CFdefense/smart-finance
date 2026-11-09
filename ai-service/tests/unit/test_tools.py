"""Tests for the analyzer's read-only tools. The database layer is replaced with fakes."""

from datetime import date

import agent.tools.transactions as transactions_mod
from agent import AnalyzedTransaction
from agent.tools import ALL_TOOLS
from db import Transaction

TXN = AnalyzedTransaction(
    transaction_id="t-now",
    account_id="acc-1",
    name="Casino Royale",
    amount=250.0,
    date=date(2026, 11, 4),
    pending=False,
    category="ENTERTAINMENT",
)
STATE = {"user_id": 7, "transaction": TXN}


def test_no_tool_exposes_user_id_to_the_model():
    """The model can pass only business arguments; user and transaction come from state."""
    for tool in ALL_TOOLS:
        fields = set(tool.tool_call_schema.model_fields)
        assert "user_id" not in fields
        assert "state" not in fields


def test_tool_names():
    """The model's only tool is get_transactions; rules, budgets and goals are always loaded."""
    assert [t.name for t in ALL_TOOLS] == ["get_transactions"]


def test_get_transactions_filters_category_and_skips_current(monkeypatch):
    """Recent transactions for the injected user; the current one and other categories drop out."""
    seen = {}

    def fake_load(user_id, since=None, limit=None):
        seen.update(user_id=user_id, since=since, limit=limit)
        return [
            Transaction(transaction_id="t-now", name="Casino Royale", amount=250, date=TXN.date),
            Transaction(
                transaction_id="t1",
                name="Lunch",
                amount=15,
                date=date(2026, 11, 3),
                category="FOOD_DINING",
            ),
            Transaction(
                transaction_id="t2",
                name="Bet",
                amount=40,
                date=date(2026, 11, 2),
                category="ENTERTAINMENT",
            ),
        ]

    monkeypatch.setattr(transactions_mod, "load_transactions", fake_load)
    monkeypatch.setattr(
        transactions_mod, "date", type("D", (), {"today": staticmethod(lambda: date(2026, 11, 4))})
    )

    text = transactions_mod.get_transactions.invoke(
        {"days": 10, "category": "ENTERTAINMENT", "limit": 5, "state": STATE}
    )

    assert seen["user_id"] == 7
    assert "Bet" in text and "Lunch" not in text and "Casino Royale" not in text
