"""Format only the user's accounts, flagging the one a transaction was made on."""

from datetime import date

from agent.nodes.context.format import _money, _section
from agent.safety import sanitize
from db import Account


def format_accounts_context(
    today: date, accounts: list[Account], transaction_account_id: str
) -> str:
    """Return today's date and the user's accounts as prompt text."""
    lines = [
        f"- {sanitize(a.name, 60)[0]} ({a.account_subtype or a.account_type}): "
        f"current {_money(a.current_balance)}, available {_money(a.available_balance)}"
        + (" <- this transaction" if a.account_id == transaction_account_id else "")
        for a in accounts
    ]
    return f"Today is {today.isoformat()}.\n\n" + _section("Accounts", lines)
