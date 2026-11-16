"""Tools: totals, comparisons and projections computed in code, so the model need not add."""

from datetime import date, timedelta

from langchain_core.tools import tool

from agent import analytics as calc
from agent.data_models import Category
from agent.formatting import money, section
from agent.safety import sanitize
from agent.tools.formatting import Refusal, State, cap, safe_tool, today
from db import (
    category_totals,
    load_accounts,
    load_budgets,
    load_goals,
    load_transactions,
    merchant_totals,
    net_flow,
)

_SAVINGS_WINDOW_DAYS = 90
_SAVINGS_MONTHS = 3
_RECURRING_WINDOW_DAYS = 180
_RECURRING_FETCH = 1000
_NAME_MAX = 60


def _window(days: int) -> tuple[int, date, date]:
    """Return (days, start, end) for the last `days` days including today; end is exclusive."""
    days = max(1, min(days, 365))
    end = today() + timedelta(days=1)
    return days, end - timedelta(days=days), end


@tool
@safe_tool
def spending_by_category(state: State, days: int = 30) -> str:
    """Total money spent per category over the last `days` days (default 30), biggest first."""
    days, start, end = _window(days)
    totals = category_totals(state["user_id"], start, end)
    lines = [f"- {category}: {money(total)}" for category, total in totals.items()]
    return cap(
        section(f"Money spent in the last {days} days (total {money(sum(totals.values()))})", lines)
    )


@tool
@safe_tool
def spending_by_merchant(state: State, days: int = 30, limit: int = 10) -> str:
    """Where the user spent the most over the last `days` days: top merchants with totals."""
    days, start, end = _window(days)
    limit = max(1, min(limit, 25))
    lines = [
        f"- {sanitize(m.name, _NAME_MAX)[0]}: {money(m.total)} "
        f"({m.count} purchase{'s' if m.count != 1 else ''})"
        for m in merchant_totals(state["user_id"], start, end, limit)
    ]
    return cap(section(f"Top merchants in the last {days} days", lines))


@tool
@safe_tool
def compare_periods(state: State, days: int = 30) -> str:
    """Compare spending per category in the last `days` days with the same number of days before."""
    days, start, end = _window(days)
    now = category_totals(state["user_id"], start, end)
    before = category_totals(state["user_id"], start - timedelta(days=days), start)
    categories = sorted(set(now) | set(before), key=lambda c: (-now.get(c, 0.0), c))
    lines = [
        f"- {c}: {money(now.get(c, 0.0))} vs {money(before.get(c, 0.0))} "
        f"({calc.describe_change(now.get(c, 0.0), before.get(c, 0.0))})"
        for c in categories
    ]
    title = (
        f"Last {days} days vs the {days} days before "
        f"(total {money(sum(now.values()))} vs {money(sum(before.values()))})"
    )
    return cap(section(title, lines))


@tool
@safe_tool
def budget_status(state: State) -> str:
    """Show each budget's spend this month, what is left, the projected month-end spend and status."""
    now = today()
    lines = [
        f"- {sanitize(s.name, _NAME_MAX)[0]} [{s.category}]: {money(s.spent)} of {money(s.limit)}, "
        f"{money(s.remaining)} left, projected {money(s.projected)} - {s.state}"
        for s in calc.budget_status(load_budgets(state["user_id"], now), now)
    ]
    return cap(section(f"Budget status for {now:%B %Y}", lines))


@tool
@safe_tool
def goal_progress(state: State) -> str:
    """Show each savings goal's progress and an estimated finish date from recent savings."""
    uid = state["user_id"]
    now = today()
    goals = load_goals(uid)
    flow = (
        net_flow(uid, now - timedelta(days=_SAVINGS_WINDOW_DAYS), now + timedelta(days=1))
        if goals
        else 0.0
    )
    monthly = flow / _SAVINGS_MONTHS
    lines = []
    for goal in goals:
        p = calc.goal_progress(goal, now, monthly)
        if p.eta is None:
            pace = "finish date unknown (no positive savings rate lately)"
        else:
            pace = f"saving about {money(monthly)}/month, finishing around {p.eta.isoformat()}"
        verdict = {True: " - on track", False: " - behind", None: ""}[p.on_track]
        due = f" (target {goal.target_date.isoformat()})" if goal.target_date else ""
        lines.append(
            f"- [id={goal.id if goal.id is not None else '?'}] {sanitize(goal.name, _NAME_MAX)[0]}: "
            f"{p.percent:.0f}% done, {money(p.remaining)} to go; {pace}{due}{verdict}"
        )
    return cap(section("Goal progress", lines))


@tool
@safe_tool
def find_recurring_charges(state: State) -> str:
    """Find subscriptions and other repeating weekly or monthly charges in the last six months."""
    now = today()
    rows = load_transactions(
        state["user_id"],
        since=now - timedelta(days=_RECURRING_WINDOW_DAYS),
        limit=_RECURRING_FETCH,
    )
    lines = [
        f"- {sanitize(r.name, _NAME_MAX)[0]}: {money(r.amount)} {r.cadence} ({r.count} charges), "
        f"last {r.last_seen.isoformat()}, next about {r.next_expected.isoformat()}"
        for r in calc.find_recurring(rows)
    ]
    return cap(section("Recurring charges", lines))


@tool
@safe_tool
def can_i_afford(state: State, amount: float, category: Category | None = None) -> str:
    """
    Check whether a purchase of `amount` fits the user's spendable balance.

    If `category` is given, also check what is left in that category's monthly budget.
    """
    if amount <= 0:
        raise Refusal("amount must be greater than 0.")
    uid = state["user_id"]
    accounts = [a for a in load_accounts(uid) if a.account_type == "depository"]
    if not accounts:
        raise Refusal(
            "The user has no checking or savings account linked, so affordability cannot be checked."
        )
    balance = sum(
        a.available_balance if a.available_balance is not None else (a.current_balance or 0.0)
        for a in accounts
    )
    remaining = None
    if category is not None:
        budget = next((b for b in load_budgets(uid, today()) if b.category == category), None)
        if budget is not None:
            remaining = budget.monthly_limit - budget.spent_this_month
    result = calc.can_afford(amount, balance, remaining, category)
    return cap("\n".join([f"Verdict: {result.verdict}", *result.reasons]))


ANALYTICS_TOOLS = [
    spending_by_category,
    spending_by_merchant,
    compare_periods,
    budget_status,
    goal_progress,
    find_recurring_charges,
    can_i_afford,
]
