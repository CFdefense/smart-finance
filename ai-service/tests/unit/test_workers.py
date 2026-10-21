"""Tests for check_transactions. These use a fake detector, so they don't depend on the rule."""

from datetime import date

import monitor.workers as workers
from monitor.data_models import FlaggedTransaction, Transaction


def txn(tid: str, amount: float) -> Transaction:
    """Build a test transaction with only the fields that matter."""
    return Transaction(transaction_id=tid, name="Store", amount=amount, date=date(2026, 10, 1))


class FakeDetector:
    """Records what it was given and flags every new transaction."""

    def find_anomalies(self, new, history):
        """Remember the inputs and flag everything that is new."""
        self.new, self.history = new, history
        return [FlaggedTransaction(transaction=t, reason="fake") for t in new]


def test_check_splits_new_transactions_from_history(monkeypatch):
    """check_transactions sends the requested IDs as 'new' and everything else as history."""
    fake = FakeDetector()
    monkeypatch.setattr(workers, "detector", fake)
    monkeypatch.setattr(
        workers,
        "load_transactions",
        lambda user_id: [txn("old1", 10), txn("old2", 20), txn("t4", 900)],
    )

    flagged = workers.check_transactions(7, ["t4"])

    assert [t.transaction_id for t in fake.new] == ["t4"]
    assert [t.transaction_id for t in fake.history] == ["old1", "old2"]
    assert [f.transaction.transaction_id for f in flagged] == ["t4"]


def test_unknown_ids_are_ignored(monkeypatch):
    """IDs that aren't this user's transactions are never checked."""
    fake = FakeDetector()
    monkeypatch.setattr(workers, "detector", fake)
    monkeypatch.setattr(workers, "load_transactions", lambda user_id: [txn("old1", 10)])

    assert workers.check_transactions(7, ["someone_elses_txn"]) == []
