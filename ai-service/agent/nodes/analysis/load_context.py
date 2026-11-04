from datetime import date

from agent.logging import traced_node
from agent.nodes.context import format_accounts_context
from agent.state import AnalyzerState
from db import load_accounts


@traced_node("load_context")
def load_account_context(state: AnalyzerState) -> dict:
    """Summarize the user's accounts and balances as text. No LLM involved."""
    accounts = load_accounts(state["user_id"])
    context = format_accounts_context(date.today(), accounts, state["transaction"].account_id)
    return {"context": context}
