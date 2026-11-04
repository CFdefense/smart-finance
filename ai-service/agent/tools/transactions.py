"""Tool: recent transactions for the user under analysis."""

from datetime import date, timedelta
from typing import Annotated

from langchain_core.tools import tool
from langgraph.prebuilt import InjectedState

from agent.safety import sanitize
from agent.tools.formatting import cap, money
from db import load_transactions

_FETCH_LIMIT = 200


@tool
def get_transactions(
    state: Annotated[dict, InjectedState],
    days: int = 30,
    category: str | None = None,
    limit: int = 20,
) -> str:
    """
    List the user's recent transactions (newest first), optionally only one category.

    Use this to compare the current transaction with the user's other spending.
    """
    days = max(1, min(days, 90))
    limit = max(1, min(limit, 50))
    current_id = state["transaction"].transaction_id
    rows = load_transactions(
        state["user_id"], since=date.today() - timedelta(days=days), limit=_FETCH_LIMIT
    )
    rows = [
        t
        for t in rows
        if t.transaction_id != current_id and (category is None or t.category == category)
    ][:limit]
    lines = [
        f"- {t.date.isoformat()} {sanitize(t.name, 60)[0]}: {money(t.amount)} [{t.category or 'OTHER'}]"
        for t in rows
    ]
    return cap(f"Transactions in the last {days} days:\n" + ("\n".join(lines) or "- none"))
