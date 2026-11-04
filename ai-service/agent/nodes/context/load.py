"""Graph node that loads a user's financial data into the agent state."""

from datetime import date, timedelta

from agent.logging import traced_node
from agent.nodes.context.format import MAX_TRANSACTIONS, RECENT_DAYS, format_financial_context
from agent.state import ChatState
from db import load_accounts, load_budgets, load_goals, load_transactions


def load_financial_context(user_id: int) -> str:
    """Load one user's accounts, budgets, goals and recent transactions as prompt text."""
    today = date.today()
    return format_financial_context(
        today,
        accounts=load_accounts(user_id),
        budgets=load_budgets(user_id, today),
        goals=load_goals(user_id),
        transactions=load_transactions(
            user_id, since=today - timedelta(days=RECENT_DAYS), limit=MAX_TRANSACTIONS
        ),
    )


@traced_node("load_context")
def load_context(state: ChatState) -> dict:
    """Look up the user's financial data and summarize it as text."""
    return {"context": load_financial_context(state["user_id"])}
