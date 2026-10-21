"""Tests for the placeholder MedianDetector. Replace this file when the detector is replaced."""

from datetime import date

import monitor.detector as detector_module
from monitor.data_models import FlaggedTransaction, Transaction
from monitor.detector import MedianDetector

detector = MedianDetector()


def txn(tid: str, amount: float) -> Transaction:
    """Build a test transaction with only the fields that matter."""
    return Transaction(transaction_id=tid, name="Store", amount=amount, date=date(2026, 10, 1))


HISTORY = [txn("a", 20), txn("b", 25), txn("c", 30)]  # typical (median) = 25


class FakeDetector:
    """Records what it was given and flags every new transaction."""

    def find_anomalies(self, new, history):
        """Remember the inputs and flag everything that is new."""
        self.new, self.history = new, history
        return [FlaggedTransaction(transaction=t, reason="fake") for t in new]


def test_check_splits_new_transactions_from_history(monkeypatch):
    """check_transactions sends the requested IDs as 'new' and everything else as history."""
    fake = FakeDetector()
    monkeypatch.setattr(detector_module, "detector", fake)
    monkeypatch.setattr(
        detector_module,
        "load_transactions",
        lambda user_id: [txn("old1", 10), txn("old2", 20), txn("t4", 900)],
    )

    flagged = detector_module.check_transactions(7, ["t4"])

    assert [t.transaction_id for t in fake.new] == ["t4"]
    assert [t.transaction_id for t in fake.history] == ["old1", "old2"]
    assert [f.transaction.transaction_id for f in flagged] == ["t4"]


def test_unknown_ids_are_ignored(monkeypatch):
    """IDs that aren't this user's transactions are never checked."""
    fake = FakeDetector()
    monkeypatch.setattr(detector_module, "detector", fake)
    monkeypatch.setattr(detector_module, "load_transactions", lambda user_id: [txn("old1", 10)])

    assert detector_module.check_transactions(7, ["someone_elses_txn"]) == []


def test_flags_new_purchase_over_three_times_typical():
    """A new purchase over 3x the typical past amount is flagged, with a reason."""
    flagged = detector.find_anomalies([txn("big", 100)], HISTORY)

    assert [f.transaction.transaction_id for f in flagged] == ["big"]
    assert "3x" in flagged[0].reason


def test_normal_new_purchase_is_not_flagged():
    """An ordinary new purchase is not flagged."""
    assert detector.find_anomalies([txn("new", 30)], HISTORY) == []


def test_money_coming_in_is_ignored():
    """Refunds and paychecks (negative amounts) are never flagged."""
    assert detector.find_anomalies([txn("paycheck", -2000)], HISTORY) == []


def test_too_little_history_flags_nothing():
    """With too few past purchases there is no 'usual' to compare against."""
    assert detector.find_anomalies([txn("big", 1000)], HISTORY[:2]) == []


def test_multiplier_is_configurable():
    """A stricter multiplier flags purchases the default would allow."""
    assert detector.find_anomalies([txn("new", 60)], HISTORY) == []
    assert MedianDetector(multiplier=2.0).find_anomalies([txn("new", 60)], HISTORY) != []
