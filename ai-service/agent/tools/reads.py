"""Tools: read the user's own data. Every query is scoped to the user in the graph state."""

from datetime import timedelta

from langchain_core.tools import tool

from agent.data_models import Category
from agent.formatting import money, section
from agent.safety import fence, sanitize
from agent.tools.formatting import State, cap, safe_tool, today
from db import load_accounts, load_all_rules, load_budgets, load_goals, load_transactions

_FETCH_LIMIT = 500
_NAME_MAX = 60


def _name(text: str | None) -> str:
    """Sanitize a name that came from the user or the bank."""
    return sanitize(text or "", _NAME_MAX)[0]


@tool
@safe_tool
def list_accounts(state: State) -> str:
    """List the user's linked bank accounts with their type and current and available balances."""
    lines = [
        f"- {_name(a.name)} ({a.account_subtype or a.account_type}): "
        f"current {money(a.current_balance)}, available {money(a.available_balance)}"
        for a in load_accounts(state["user_id"])
    ]
    return cap(section("Accounts", lines))


@tool
@safe_tool
def list_transactions(
    state: State,
    days: int = 30,
    category: Category | None = None,
    merchant: str | None = None,
    limit: int = 20,
) -> str:
    """
    List the user's transactions, newest first (positive = money out).

    Optionally limit to one category or to merchants whose name contains `merchant`.
    Use it to look up individual purchases; use the analytics tools for totals.
    """
    days = max(1, min(days, 365))
    limit = max(1, min(limit, 50))
    rows = load_transactions(
        state["user_id"], since=today() - timedelta(days=days), limit=_FETCH_LIMIT
    )
    wanted = (merchant or "").strip().lower()
    rows = [
        t for t in rows if (category is None or t.category == category) and wanted in t.name.lower()
    ][:limit]
    lines = [
        f"- {t.date.isoformat()} {_name(t.name)}: {money(t.amount)} [{t.category or 'OTHER'}]"
        for t in rows
    ]
    return cap(section(f"Transactions in the last {days} days", lines))


@tool
@safe_tool
def list_budgets(state: State) -> str:
    """List the user's active monthly budgets with ids and how much is spent this month."""
    now = today()
    lines = [
        f"- [id={b.id}] {_name(b.name)} [{b.category}]: spent {money(b.spent_this_month)} "
        f"of {money(b.monthly_limit)}"
        for b in load_budgets(state["user_id"], now)
    ]
    return cap(section(f"Budgets for {now:%B %Y}", lines))


@tool
@safe_tool
def list_goals(state: State) -> str:
    """List the user's savings goals with ids, balances, targets and progress."""
    lines = []
    for g in load_goals(state["user_id"]):
        current = g.baseline_balance if g.current_balance is None else g.current_balance
        span = g.target_amount - g.baseline_balance
        done = 100 if span <= 0 else max(0, min(100, (current - g.baseline_balance) / span * 100))
        lines.append(
            f"- [id={g.id if g.id is not None else '?'}] {_name(g.name)} on {_name(g.account_name)}: "
            f"balance {money(g.current_balance)}, started at {money(g.baseline_balance)}, "
            f"target {money(g.target_amount)}"
            + (f" by {g.target_date.isoformat()}" if g.target_date else "")
            + f" ({done:.0f}% done)"
        )
    return cap(section("Goals", lines))


@tool
@safe_tool
def list_rules(state: State) -> str:
    """List the user's enabled rules (their own words) with ids and the accounts they cover."""
    uid = state["user_id"]
    names = {a.account_id: _name(a.name) for a in load_accounts(uid)}
    lines = [
        f"- [id={r.id}] {fence('user_rule', r.description, 500)} "
        f"({', '.join(names.get(i, 'unknown account') for i in r.account_ids) or 'all accounts'})"
        for r in load_all_rules(uid)
    ]
    return cap(section("Rules", lines))


READ_TOOLS = [list_accounts, list_transactions, list_budgets, list_goals, list_rules]
