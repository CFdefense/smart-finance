"""Load-context node: summarize the user's accounts for the prompt."""

import logging
from datetime import date

from agent.logging import NODE, traced_node
from agent.nodes.context import format_accounts_context
from agent.state import AnalyzerState
from db import load_accounts

_log = logging.getLogger(NODE)


@traced_node("load_context")
def load_account_context(state: AnalyzerState) -> dict:
    """
    Summarize the user's accounts and balances as text. No LLM involved.

    The transaction is already claimed at this point, so a failure here must not abort the run
    (that would leave the claim stuck in progress); the analysis continues without account context.
    """
    try:
        accounts = load_accounts(state["user_id"])
    except Exception as exc:
        _log.warning("load_context.failed continuing without accounts type=%s", type(exc).__name__)
        _log.debug("load_context.failed detail", exc_info=True)
        return {"context": "Account context unavailable."}
    context = format_accounts_context(date.today(), accounts, state["transaction"].account_id)
    return {"context": context}
