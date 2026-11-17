"""Tools: totals, comparisons and projections computed in code, so the model need not add."""

import math
from datetime import timedelta

from langchain_core.tools import tool

from agent import analytics as calc
from agent.data_models import Category
from agent.formatting import money, section
from agent.tools.helpers import (
    MAX_AMOUNT,
    Refusal,
    State,
    budget_in,
    cap,
    depository_accounts,
    goal_balance,
    label,
    safe_tool,
    spendable,
    today,
    window,
)
from db import (
    category_totals,
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


@tool
@safe_tool
def spending_by_category(state: State, days: int = 30) -> str:
    """Total money spent per category over the last `days` days (default 30), biggest first."""
    days, start, end = window(days)
    totals = category_totals(state["user_id"], start, end)
    lines = [f"- {category}: {money(total)}" for category, total in totals.items()]
    return cap(
        section(f"Money spent in the last {days} days (total {money(sum(totals.values()))})", lines)
    )


@tool
@safe_tool
def spending_by_merchant(state: State, days: int = 30, limit: int = 10) -> str:
    """Where the user spent the most over the last `days` days: top merchants with totals."""
    days, start, end = window(days)
    limit = max(1, min(limit, 25))
    lines = [
        f"- {label(m.name)}: {money(m.total)} ({m.count} purchase{'s' if m.count != 1 else ''})"
        for m in merchant_totals(state["user_id"], start, end, limit)
    ]
    return cap(section(f"Top merchants in the last {days} days", lines))


@tool
@safe_tool
def compare_periods(state: State, days: int = 30) -> str:
    """Compare spending per category in the last `days` days with the same number of days before."""
    days, start, end = window(days)
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
    """Show each budget's spend this month, what is left, the month-end projection and status."""
    now = today()
    lines = [
        f"- {label(s.name)} [{s.category}]: {money(s.spent)} of {money(s.limit)}, "
        f"{money(s.remaining)} left, projected {money(s.projected)} - {s.state}"
        for s in calc.budget_status(load_budgets(state["user_id"], now), now)
    ]
    return cap(section(f"Budget status for {now:%B %Y}", lines))


@tool
@safe_tool
def goal_progress(state: State) -> str:
    """Show each savings goal's progress and a finish date from its account's net deposits."""
    uid = state["user_id"]
    now = today()
    start, end = now - timedelta(days=_SAVINGS_WINDOW_DAYS), now + timedelta(days=1)
    lines = []
    for goal in load_goals(uid):
        # A goal tracks one account's balance, so its pace is that account's net deposits.
        monthly = net_flow(uid, start, end, goal.account_id) / _SAVINGS_MONTHS
        p = calc.goal_progress(goal, now, monthly)
        if p.eta is None:
            pace = "finish date unknown (no net deposits into this account lately)"
        else:
            pace = (
                f"net deposits into this account about {money(monthly)}/month "
                f"(last {_SAVINGS_WINDOW_DAYS} days), finishing around {p.eta.isoformat()}"
            )
        verdict = {True: " - on track", False: " - behind", None: ""}[p.on_track]
        due = f" (target {goal.target_date.isoformat()})" if goal.target_date else ""
        goal_id = goal.id if goal.id is not None else "?"
        lines.append(
            f"- [id={goal_id}] {label(goal.name)}: "
            f"{p.percent:.0f}% done, {money(p.remaining)} to go of {money(goal.target_amount)} "
            f"(balance {money(goal_balance(goal))}, {money(goal.baseline_balance)} when the goal was "
            f"set); {pace}{due}{verdict}"
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
        f"- {label(r.name)}: {money(r.amount)} {r.cadence} ({r.count} charges), "
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
    if not math.isfinite(amount) or not 0 < amount <= MAX_AMOUNT:
        raise Refusal(f"amount must be greater than 0 and at most {money(MAX_AMOUNT)}.")
    uid = state["user_id"]
    accounts = depository_accounts(uid)
    if not accounts:
        raise Refusal("The user has no checking or savings account, so affordability is unknown.")
    balance = sum(spendable(a) for a in accounts)
    remaining = None
    budget = budget_in(uid, category) if category is not None else None
    if budget is not None:
        remaining = budget.monthly_limit - budget.spent_this_month
    result = calc.can_afford(amount, balance, remaining, category)
    counted = ", ".join(f"{label(a.name)} {money(spendable(a))}" for a in accounts)
    note = f"Spendable balance is the available balance of these accounts: {counted}."
    return cap("\n".join([f"Verdict: {result.verdict}", *result.reasons, note]))


ANALYTICS_TOOLS = [
    spending_by_category,
    spending_by_merchant,
    compare_periods,
    budget_status,
    goal_progress,
    find_recurring_charges,
    can_i_afford,
]
