"""Format what the user has set up (rules, budgets, goals) as prompt text."""

from datetime import date

from agent.nodes.context.format import _money, _section
from agent.safety import fence, sanitize
from agent.tools.formatting import cap
from db import Budget, Goal, Rule


def format_rules(rules: list[Rule]) -> str:
    """List the rules that apply to the transaction's account. Rule text is fenced as data."""
    lines = [f"- {fence('user_rule', r.description, 500)}" for r in rules]
    return cap(_section("Rules that apply to this account", lines))


def format_budgets(today: date, budgets: list[Budget]) -> str:
    """List the active monthly budgets and how much of each is spent so far this month."""
    lines = [
        f"- {sanitize(b.name, 60)[0]} [{b.category}]: spent {_money(b.spent_this_month)} "
        f"of {_money(b.monthly_limit)}"
        for b in budgets
    ]
    return cap(_section(f"Budgets for {today:%B %Y}", lines))


def format_goals(goals: list[Goal]) -> str:
    """List the savings goals with the current balance, starting balance and target."""
    lines = [
        f"- {sanitize(g.name, 60)[0]} on {sanitize(g.account_name, 60)[0]}: "
        f"balance {_money(g.current_balance)}, started at {_money(g.baseline_balance)}, "
        f"target {_money(g.target_amount)}"
        + (f" by {g.target_date.isoformat()}" if g.target_date else "")
        for g in goals
    ]
    return cap(_section("Goals", lines))
