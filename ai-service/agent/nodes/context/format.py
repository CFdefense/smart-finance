"""Format a user's finances as plain text for the chat agent's prompt."""

from datetime import date

from db import Account, Budget, Goal, Transaction

RECENT_DAYS = 30
MAX_TRANSACTIONS = 100


def format_financial_context(
    today: date,
    accounts: list[Account],
    budgets: list[Budget],
    goals: list[Goal],
    transactions: list[Transaction],
) -> str:
    """Format a user's data as compact sections the model can read."""
    sections = [f"Today is {today.isoformat()}."]
    sections.append(
        _section(
            "Accounts",
            [
                f"- {a.name} ({a.account_subtype or a.account_type}): "
                f"current {_money(a.current_balance)}, available {_money(a.available_balance)}"
                for a in accounts
            ],
        )
    )
    sections.append(
        _section(
            f"Budgets for {today:%B %Y}",
            [
                f"- {b.name} [{b.category}]: spent {_money(b.spent_this_month)} "
                f"of {_money(b.monthly_limit)}"
                for b in budgets
            ],
        )
    )
    sections.append(
        _section(
            "Goals",
            [
                f"- {g.name} on {g.account_name}: balance {_money(g.current_balance)}, "
                f"started at {_money(g.baseline_balance)}, target {_money(g.target_amount)}"
                + (f" by {g.target_date.isoformat()}" if g.target_date else "")
                + (f" ({g.description})" if g.description else "")
                for g in goals
            ],
        )
    )
    title = f"Transactions in the last {RECENT_DAYS} days (positive = money out)"
    if len(transactions) >= MAX_TRANSACTIONS:
        title += f", newest {MAX_TRANSACTIONS} only"
    sections.append(
        _section(
            title,
            [
                f"- {t.date.isoformat()} {t.name}: {_money(t.amount)} [{t.category or 'OTHER'}]"
                for t in transactions
            ],
        )
    )
    return "\n\n".join(sections)


def _section(title: str, lines: list[str]) -> str:
    """Format a section with a title and bullet points, or say 'none' if empty."""
    return f"{title}:\n" + ("\n".join(lines) if lines else "- none")


def _money(amount: float | None) -> str:
    """Return a formatted string for a monetary amount, saying 'unknown' if None."""
    if amount is None:
        return "unknown"
    sign = "-" if amount < 0 else ""
    return f"{sign}${abs(amount):,.2f}"
