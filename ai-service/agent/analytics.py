"""Pure analytics over the user's data. No database and no clock: callers pass everything in."""

import calendar
import math
import re
import statistics
from collections import defaultdict
from dataclasses import dataclass
from datetime import date, timedelta

from agent.formatting import money
from db import Budget, Goal, Transaction

LOW_BALANCE = 100.0
_DAYS_PER_MONTH = 30.44
_PRICE_TOLERANCE = 0.10
_GAP_TOLERANCE_DAYS = 3
_NON_NAME = re.compile(r"[\d#*]+")
_SPACE = re.compile(r"\s+")


@dataclass(frozen=True)
class BudgetStatus:
    """Where one budget stands this month."""

    name: str
    category: str
    limit: float
    spent: float
    remaining: float
    percent_used: float
    projected: float
    state: str  # "over", "at risk" (projected to go over) or "on track"


@dataclass(frozen=True)
class GoalProgress:
    """How far along a goal is and when it should finish at the recent savings rate."""

    name: str
    percent: float
    remaining: float
    monthly_savings: float
    eta: date | None  # None when the savings rate is not positive
    on_track: bool | None  # None when the goal has no target date


@dataclass(frozen=True)
class Recurring:
    """A charge that repeats on a regular schedule."""

    name: str
    amount: float
    cadence: str  # "weekly" or "monthly"
    count: int
    last_seen: date
    next_expected: date


@dataclass(frozen=True)
class Affordability:
    """Whether a purchase fits, and why."""

    verdict: str  # "yes", "tight" or "no"
    reasons: list[str]
    balance_after: float
    budget_remaining: float | None


def budget_status(budgets: list[Budget], today: date) -> list[BudgetStatus]:
    """Project each budget's month-end spend from the pace so far this month."""
    days_in_month = calendar.monthrange(today.year, today.month)[1]
    results = []
    for b in budgets:
        projected = b.spent_this_month / today.day * days_in_month
        if b.spent_this_month > b.monthly_limit:
            state = "over"
        elif projected > b.monthly_limit:
            state = "at risk"
        else:
            state = "on track"
        percent = b.spent_this_month / b.monthly_limit * 100 if b.monthly_limit > 0 else 0.0
        results.append(
            BudgetStatus(
                name=b.name,
                category=b.category,
                limit=b.monthly_limit,
                spent=b.spent_this_month,
                remaining=b.monthly_limit - b.spent_this_month,
                percent_used=percent,
                projected=projected,
                state=state,
            )
        )
    return results


def goal_progress(goal: Goal, today: date, monthly_savings: float) -> GoalProgress:
    """Compute progress and an ETA from the trailing monthly savings rate."""
    current = goal.baseline_balance if goal.current_balance is None else goal.current_balance
    span = goal.target_amount - goal.baseline_balance
    percent = (
        100.0 if span <= 0 else max(0.0, min(100.0, (current - goal.baseline_balance) / span * 100))
    )
    remaining = max(goal.target_amount - current, 0.0)
    if remaining == 0:
        eta: date | None = today
    elif monthly_savings > 0:
        eta = today + timedelta(days=math.ceil(remaining / monthly_savings * _DAYS_PER_MONTH))
    else:
        eta = None
    on_track = None if goal.target_date is None else (eta is not None and eta <= goal.target_date)
    return GoalProgress(
        name=goal.name,
        percent=percent,
        remaining=remaining,
        monthly_savings=monthly_savings,
        eta=eta,
        on_track=on_track,
    )


def _merchant_key(name: str) -> str:
    """Normalize a merchant name so 'SPOTIFY 123' and 'Spotify #9' group together."""
    return _SPACE.sub(" ", _NON_NAME.sub("", name.lower())).strip()


def find_recurring(transactions: list[Transaction], min_count: int = 3) -> list[Recurring]:
    """
    Find weekly or monthly charges: at least `min_count` spends at one merchant.

    Amounts must be within 10% of their median and the gaps between charges must be regular.
    """
    groups: dict[str, list[Transaction]] = defaultdict(list)
    for t in transactions:
        if t.amount > 0:
            groups[_merchant_key(t.name)].append(t)
    found = []
    for key, items in groups.items():
        if not key or len(items) < min_count:
            continue
        items.sort(key=lambda t: t.date)
        typical = statistics.median(t.amount for t in items)
        if any(abs(t.amount - typical) > typical * _PRICE_TOLERANCE for t in items):
            continue
        gaps = [(b.date - a.date).days for a, b in zip(items, items[1:], strict=False)]
        gap = statistics.median(gaps)
        if any(abs(g - gap) > _GAP_TOLERANCE_DAYS for g in gaps):
            continue
        if 6 <= gap <= 8:
            cadence, step = "weekly", 7
        elif 27 <= gap <= 34:
            cadence, step = "monthly", 30
        else:
            continue
        last = items[-1]
        found.append(
            Recurring(
                name=last.name,
                amount=round(typical, 2),
                cadence=cadence,
                count=len(items),
                last_seen=last.date,
                next_expected=last.date + timedelta(days=step),
            )
        )
    return sorted(found, key=lambda r: -r.amount)


def can_afford(
    amount: float, balance: float, budget_remaining: float | None, category: str | None
) -> Affordability:
    """Judge a purchase against the spendable balance and, if known, the category budget."""
    after = balance - amount
    reasons = [f"Spendable balance {money(balance)}; after this purchase {money(after)}."]
    verdict = "yes"
    if after < 0:
        verdict = "no"
        reasons.append("The purchase is more than the spendable balance.")
    elif after < LOW_BALANCE:
        verdict = "tight"
        reasons.append(f"It would leave under {money(LOW_BALANCE)}.")
    if budget_remaining is not None and amount > budget_remaining:
        verdict = "no" if verdict == "no" else "tight"
        label = category or "the"
        reasons.append(
            f"It is {money(amount - budget_remaining)} over what is left in the {label} budget "
            f"({money(max(budget_remaining, 0))})."
        )
    return Affordability(
        verdict=verdict, reasons=reasons, balance_after=after, budget_remaining=budget_remaining
    )


def describe_change(current: float, previous: float) -> str:
    """Describe a change as a signed amount and percentage, or 'new this period' / 'no change'."""
    if previous == 0:
        return "new this period" if current > 0 else "no change"
    diff = current - previous
    if diff == 0:
        return "no change"
    sign = "+" if diff > 0 else "-"
    return f"{sign}{money(abs(diff))}, {diff / previous * 100:+.0f}%"
