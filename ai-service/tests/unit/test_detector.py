"""Tests for the anomaly detectors and check_transactions."""

from datetime import date

import anomaly.detector as detector_module
from anomaly.data_models import AnomalyRequest, FlaggedTransaction, Transaction
from anomaly.detector import (
    CombinedDetector,
    DuplicateDetector,
    SubscriptionIncreaseDetector,
    ZScoreDetector,
)


def txn(
    tid: str,
    amount: float,
    name: str = "Store",
    on: date = date(2026, 10, 1),
    category: str | None = None,
) -> Transaction:
    """Build a test transaction with only the fields that matter."""
    return Transaction(transaction_id=tid, name=name, amount=amount, date=on, category=category)


def flagged_ids(flagged: list[FlaggedTransaction]) -> list[str]:
    """The IDs of the flagged transactions, in order."""
    return [f.transaction.transaction_id for f in flagged]


class FakeDetector:
    """Records what it was given and flags every new transaction."""

    def __init__(self, reason: str = "fake"):
        self.reason = reason

    def find_anomalies(self, new, history):
        """Remember the inputs and flag everything that is new."""
        self.new, self.history = new, history  # Save new vs history to make sure they're right.
        return [FlaggedTransaction(transaction=t, reason=self.reason) for t in new]


# ── check_transactions ───────────────────────────────────────────────────────


def test_check_splits_new_transactions_from_history(monkeypatch):
    """check_transactions sends the requested IDs as 'new' and everything else as history."""
    fake = FakeDetector()
    monkeypatch.setattr(
        detector_module,
        "load_transactions",
        lambda user_id: [txn("old1", 10), txn("old2", 20), txn("t4", 900)],
    )

    response = detector_module.check_transactions(
        AnomalyRequest(user_id=7, transaction_ids=["t4"]), fake
    )

    assert [t.transaction_id for t in fake.new] == ["t4"]
    assert [t.transaction_id for t in fake.history] == ["old1", "old2"]
    assert flagged_ids(response.flagged) == ["t4"]


def test_unknown_ids_are_ignored(monkeypatch):
    """IDs that aren't this user's transactions are never checked."""
    fake = FakeDetector()
    monkeypatch.setattr(detector_module, "load_transactions", lambda user_id: [txn("old1", 10)])

    request = AnomalyRequest(user_id=7, transaction_ids=["someone_elses_txn"])
    assert detector_module.check_transactions(request, fake).flagged == []


# ── ZScoreDetector ───────────────────────────────────────────────────────────

zscore = ZScoreDetector()

# Median 25, median absolute deviation 3.
HISTORY = [txn("a", 20), txn("b", 25), txn("c", 30), txn("d", 22), txn("e", 28)]


def test_zscore_flags_purchase_far_above_typical():
    """A purchase far above the usual amount is flagged, with a reason."""
    flagged = zscore.find_anomalies([txn("big", 100)], HISTORY)

    assert flagged_ids(flagged) == ["big"]
    assert "typical purchase of $25.00" in flagged[0].reason


def test_zscore_ignores_normal_purchase():
    """An ordinary purchase is not flagged."""
    assert zscore.find_anomalies([txn("new", 30)], HISTORY) == []


def test_zscore_ignores_money_coming_in():
    """Refunds and paychecks (negative amounts) are never flagged."""
    assert zscore.find_anomalies([txn("paycheck", -2000)], HISTORY) == []


def test_zscore_needs_enough_history():
    """With too few past purchases there is no 'usual' to compare against."""
    assert zscore.find_anomalies([txn("big", 1000)], HISTORY[:2]) == []


def test_zscore_threshold_is_configurable():
    """A stricter threshold flags purchases the default allows."""
    assert zscore.find_anomalies([txn("new", 40)], HISTORY) == []
    assert ZScoreDetector(threshold=3.0).find_anomalies([txn("new", 40)], HISTORY) != []


def test_zscore_works_when_most_past_purchases_are_identical():
    """Mostly identical history (median absolute deviation of 0) still flags big purchases."""
    history = [txn("a", 25), txn("b", 25), txn("c", 25), txn("d", 25), txn("e", 40)]

    assert flagged_ids(zscore.find_anomalies([txn("big", 100)], history)) == ["big"]
    assert zscore.find_anomalies([txn("new", 30)], history) == []


COFFEE_AND_GROCERIES = [
    txn("c1", 4, category="COFFEE"),
    txn("c2", 5, category="COFFEE"),
    txn("c3", 6, category="COFFEE"),
    txn("g1", 100, category="GROCERIES"),
    txn("g2", 110, category="GROCERIES"),
    txn("g3", 120, category="GROCERIES"),
]


def test_zscore_compares_within_category_when_known():
    """$40 is normal overall but huge for coffee, so it's flagged when the category is known."""
    flagged = zscore.find_anomalies([txn("latte", 40, category="COFFEE")], COFFEE_AND_GROCERIES)

    assert flagged_ids(flagged) == ["latte"]
    assert "typical COFFEE purchase" in flagged[0].reason


def test_zscore_uses_whole_history_without_category():
    """Without a category, $40 is compared to everything and looks normal."""
    assert zscore.find_anomalies([txn("latte", 40)], COFFEE_AND_GROCERIES) == []


# ── DuplicateDetector ────────────────────────────────────────────────────────

duplicates = DuplicateDetector()


def test_duplicate_flags_same_charge_days_apart():
    """The same name and amount two days apart is flagged."""
    history = [txn("first", 50, name="Gym", on=date(2026, 10, 1))]
    flagged = duplicates.find_anomalies(
        [txn("again", 50, name="Gym", on=date(2026, 10, 3))], history
    )

    assert flagged_ids(flagged) == ["again"]
    assert "Oct 01" in flagged[0].reason


def test_duplicate_ignores_charges_four_or_more_days_apart():
    """The same charge four days later is not a duplicate."""
    history = [txn("first", 50, name="Gym", on=date(2026, 10, 1))]
    new = [txn("later", 50, name="Gym", on=date(2026, 10, 5))]

    assert duplicates.find_anomalies(new, history) == []


def test_duplicate_ignores_different_amounts():
    """Two charges from the same place for different amounts are not duplicates."""
    history = [txn("first", 50, name="Gym", on=date(2026, 10, 1))]
    new = [txn("other", 55, name="Gym", on=date(2026, 10, 2))]

    assert duplicates.find_anomalies(new, history) == []


def test_duplicate_within_new_charges_flags_only_the_second():
    """Two identical new charges flag only the second one."""
    new = [txn("one", 50, name="Gym"), txn("two", 50, name="Gym")]

    assert flagged_ids(duplicates.find_anomalies(new, [])) == ["two"]


# ── SubscriptionIncreaseDetector ─────────────────────────────────────────────

subscriptions = SubscriptionIncreaseDetector()


def monthly(amounts: list[float], name: str = "Netflix") -> list[Transaction]:
    """Charges on the 1st of each month starting in June."""
    return [
        txn(f"{name}-{i}", amount, name=name, on=date(2026, 6 + i, 1))
        for i, amount in enumerate(amounts)
    ]


NEXT_MONTH = date(2026, 11, 1)


def test_subscription_flags_price_increase():
    """Four monthly $10 charges followed by a $12 charge is flagged."""
    new = [txn("nov", 12, name="Netflix", on=NEXT_MONTH)]
    flagged = subscriptions.find_anomalies(new, monthly([10, 10, 10, 10]))

    assert flagged_ids(flagged) == ["nov"]
    assert "$10.00 to $12.00" in flagged[0].reason


def test_subscription_needs_more_than_three_past_charges():
    """Three past charges aren't enough to call it a subscription."""
    new = [txn("nov", 12, name="Netflix", on=NEXT_MONTH)]
    assert subscriptions.find_anomalies(new, monthly([10, 10, 10])) == []


def test_subscription_ignores_same_price():
    """Paying the usual price is not flagged."""
    new = [txn("nov", 10, name="Netflix", on=NEXT_MONTH)]
    assert subscriptions.find_anomalies(new, monthly([10, 10, 10, 10])) == []


def test_subscription_ignores_varying_past_prices():
    """Past charges that vary in price aren't a fixed subscription."""
    new = [txn("nov", 12, name="Netflix", on=NEXT_MONTH)]
    assert subscriptions.find_anomalies(new, monthly([10, 11, 10, 10])) == []


def test_subscription_ignores_irregular_dates():
    """Same-priced charges at irregular times aren't a subscription."""
    days = [date(2026, 6, 1), date(2026, 6, 5), date(2026, 7, 20), date(2026, 8, 2)]
    history = [txn(f"h{i}", 10, name="Cafe", on=d) for i, d in enumerate(days)]
    new = [txn("new", 12, name="Cafe", on=date(2026, 9, 1))]

    assert subscriptions.find_anomalies(new, history) == []


# ── CombinedDetector ─────────────────────────────────────────────────────────


def test_combined_reports_each_transaction_once_with_all_reasons():
    """A transaction flagged by two detectors appears once, with both reasons."""
    combined = CombinedDetector([FakeDetector("reason A"), FakeDetector("reason B")])
    flagged = combined.find_anomalies([txn("t1", 10), txn("t2", 20)], [])

    assert flagged_ids(flagged) == ["t1", "t2"]
    assert flagged[0].reason == "reason A; reason B"


def test_combined_flags_nothing_when_no_detector_does():
    """If no detector flags anything, neither does the combination."""
    combined = CombinedDetector([ZScoreDetector(), DuplicateDetector()])

    assert combined.find_anomalies([txn("new", 26)], HISTORY) == []
