"""Tests for the pure analytics functions."""

from datetime import date, timedelta

import pytest

from agent import analytics as calc
from db import Budget, Goal, Transaction

TODAY = date(2026, 11, 16)


def budget(spent, limit=100.0, category="FOOD_DINING", name="Eating out"):
    """Build a budget."""
    return Budget(name=name, category=category, monthly_limit=limit, spent_this_month=spent)


def goal(current, baseline=1000.0, target=3000.0, target_date=None):
    """Build a goal."""
    return Goal(
        name="Trip",
        account_name="Savings",
        target_amount=target,
        baseline_balance=baseline,
        current_balance=current,
        target_date=target_date,
    )


def txn(name, amount, days_ago, i=[0]):
    """Build a transaction `days_ago` before TODAY."""
    i[0] += 1
    return Transaction(
        transaction_id=f"t{i[0]}", name=name, amount=amount, date=TODAY - timedelta(days=days_ago)
    )


@pytest.mark.parametrize(
    ("spent", "state"),
    [(0, "on track"), (40, "on track"), (60, "at risk"), (100, "at risk"), (100.01, "over")],
)
def test_budget_status_states(spent, state):
    """Day 16 of 30: projecting spend past the limit is 'at risk'; past it already is 'over'."""
    [result] = calc.budget_status([budget(spent)], TODAY)
    assert result.state == state


def test_budget_status_numbers():
    """Remaining, percent used and the month-end projection are computed."""
    [r] = calc.budget_status([budget(40, limit=100)], TODAY)
    assert r.remaining == 60 and r.percent_used == 40
    assert r.projected == pytest.approx(40 / 16 * 30)


def test_budget_status_on_the_first_day_does_not_divide_by_zero():
    """Day 1 projects from one day of data."""
    [r] = calc.budget_status([budget(5)], date(2026, 11, 1))
    assert r.projected == pytest.approx(150)


def test_budget_status_overspent_remaining_is_negative():
    """Remaining goes negative when over."""
    [r] = calc.budget_status([budget(130, limit=100)], TODAY)
    assert r.remaining == -30 and r.percent_used == 130


def test_budget_status_empty():
    """No budgets, no statuses."""
    assert calc.budget_status([], TODAY) == []


def test_goal_progress_with_a_positive_rate():
    """ETA = remaining / monthly rate, in 30.44-day months, rounded up."""
    p = calc.goal_progress(goal(current=2000), TODAY, monthly_savings=500)
    assert p.percent == 50 and p.remaining == 1000
    assert p.eta == TODAY + timedelta(days=61)  # 2 months * 30.44 = 60.88 -> 61
    assert p.on_track is None  # no target date


def test_goal_progress_on_track_and_behind():
    """On track means the ETA is on or before the target date."""
    ok = calc.goal_progress(goal(2000, target_date=TODAY + timedelta(days=90)), TODAY, 500)
    late = calc.goal_progress(goal(2000, target_date=TODAY + timedelta(days=30)), TODAY, 500)
    assert ok.on_track is True and late.on_track is False


@pytest.mark.parametrize("rate", [0, -50])
def test_goal_progress_unknown_when_rate_is_not_positive(rate):
    """No positive savings rate means no ETA, and a dated goal is not on track."""
    p = calc.goal_progress(goal(2000, target_date=TODAY + timedelta(days=90)), TODAY, rate)
    assert p.eta is None and p.on_track is False


def test_goal_progress_already_reached():
    """A met goal has zero remaining, 100%, and an ETA of today."""
    p = calc.goal_progress(goal(3500), TODAY, monthly_savings=0)
    assert p.percent == 100 and p.remaining == 0 and p.eta == TODAY


def test_goal_progress_below_baseline_is_zero_percent():
    """Dropping below the starting balance never shows a negative percentage."""
    assert calc.goal_progress(goal(500), TODAY, 100).percent == 0


def test_goal_progress_with_unknown_balance_uses_the_baseline():
    """A missing current balance counts as no progress."""
    p = calc.goal_progress(goal(None), TODAY, 100)
    assert p.percent == 0 and p.remaining == 2000


def test_find_recurring_monthly():
    """Three charges of about the same amount ~30 days apart are a monthly subscription."""
    found = calc.find_recurring(
        [txn("NETFLIX.COM", 15.49, 5), txn("Netflix.com", 15.49, 35), txn("NETFLIX.COM", 15.49, 65)]
    )
    [r] = found
    assert (r.cadence, r.count, r.amount) == ("monthly", 3, 15.49)
    assert r.last_seen == TODAY - timedelta(days=5)
    assert r.next_expected == r.last_seen + timedelta(days=30)


def test_find_recurring_weekly_and_ordering():
    """Weekly charges are found; results are sorted by amount, biggest first."""
    found = calc.find_recurring(
        [txn("Gym", 20, d) for d in (1, 8, 15)] + [txn("Rent", 900, d) for d in (3, 33, 63)]
    )
    assert [(r.name, r.cadence) for r in found] == [("Rent", "monthly"), ("Gym", "weekly")]


def test_find_recurring_ignores_digits_and_store_numbers_in_names():
    """'STARBUCKS #123' and 'Starbucks 456' are the same merchant."""
    found = calc.find_recurring(
        [txn("SPOTIFY 1234", 11.99, 4), txn("Spotify #99", 11.99, 34), txn("SPOTIFY", 11.99, 64)]
    )
    assert len(found) == 1


@pytest.mark.parametrize(
    "items",
    [
        [txn("A", 10, 5), txn("A", 10, 35)],  # only two
        [txn("A", 10, 5), txn("A", 25, 35), txn("A", 10, 65)],  # amounts vary > 10%
        [txn("A", 10, 1), txn("A", 10, 3), txn("A", 10, 20)],  # irregular gaps
        [txn("A", -10, 5), txn("A", -10, 35), txn("A", -10, 65)],  # refunds, not spending
    ],
)
def test_find_recurring_rejects_non_subscriptions(items):
    """Too few, varying, irregular or negative charges are not recurring."""
    assert calc.find_recurring(items) == []


def test_find_recurring_tolerates_small_price_changes():
    """A price within 10% of the median still counts."""
    found = calc.find_recurring([txn("Hulu", 10, 5), txn("Hulu", 10.5, 35), txn("Hulu", 10, 65)])
    assert len(found) == 1


@pytest.mark.parametrize(
    ("amount", "balance", "remaining", "verdict"),
    [
        (50, 1000, None, "yes"),
        (50, 1000, 200, "yes"),
        (50, 1000, 20, "tight"),  # over the budget
        (950, 1000, None, "tight"),  # under $100 left
        (1000, 1000, None, "tight"),  # exactly zero left
        (1000.01, 1000, None, "no"),
        (5, 0, None, "no"),
    ],
)
def test_can_afford_verdicts(amount, balance, remaining, verdict):
    """Verdict is the worst of: negative balance, low balance, over budget."""
    assert calc.can_afford(amount, balance, remaining, "SHOPPING").verdict == verdict


def test_can_afford_reasons_mention_the_numbers():
    """Reasons show the balance after the purchase and any budget overage."""
    result = calc.can_afford(300, 1000, 100, "SHOPPING")
    text = " ".join(result.reasons)
    assert "$700.00" in text and "SHOPPING" in text and "$200.00" in text
    assert result.balance_after == 700 and result.budget_remaining == 100


@pytest.mark.parametrize(
    ("current", "previous", "text"),
    [
        (120, 80, "+$40.00, +50%"),
        (80, 120, "-$40.00, -33%"),
        (50, 0, "new this period"),
        (0, 0, "no change"),
        (0, 50, "-$50.00, -100%"),
        (100, 100, "no change"),
    ],
)
def test_describe_change(current, previous, text):
    """Changes show a signed amount and a percentage, or say new / no change."""
    assert calc.describe_change(current, previous) == text
