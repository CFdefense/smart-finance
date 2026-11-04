"""Format a user's finances as plain text for the chat agent's prompt."""

from datetime import date

from agent.formatting import money, section
from agent.safety import sanitize
from db import Account, Budget, Goal, Transaction

RECENT_DAYS = 30
MAX_TRANSACTIONS = 100
_NAME_MAX = 60
_DESCRIPTION_MAX = 200


def _clean(text: str | None) -> str:
    """Sanitize a name or description from the user or the bank before it goes in a prompt."""
    return sanitize(text or "", _NAME_MAX)[0]


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
        section(
            "Accounts",
            [
                f"- {_clean(a.name)} ({a.account_subtype or a.account_type}): "
                f"current {money(a.current_balance)}, available {money(a.available_balance)}"
                for a in accounts
            ],
        )
    )
    sections.append(
        section(
            f"Budgets for {today:%B %Y}",
            [
                f"- {_clean(b.name)} [{b.category}]: spent {money(b.spent_this_month)} "
                f"of {money(b.monthly_limit)}"
                for b in budgets
            ],
        )
    )
    sections.append(
        section(
            "Goals",
            [
                f"- {_clean(g.name)} on {_clean(g.account_name)}: balance {money(g.current_balance)}, "
                f"started at {money(g.baseline_balance)}, target {money(g.target_amount)}"
                + (f" by {g.target_date.isoformat()}" if g.target_date else "")
                + (f" ({sanitize(g.description, _DESCRIPTION_MAX)[0]})" if g.description else "")
                for g in goals
            ],
        )
    )
    title = f"Transactions in the last {RECENT_DAYS} days (positive = money out)"
    if len(transactions) >= MAX_TRANSACTIONS:
        title += f", newest {MAX_TRANSACTIONS} only"
    sections.append(
        section(
            title,
            [
                f"- {t.date.isoformat()} {_clean(t.name)}: {money(t.amount)} [{t.category or 'OTHER'}]"
                for t in transactions
            ],
        )
    )
    return "\n\n".join(sections)
