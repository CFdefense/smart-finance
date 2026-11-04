"""Claim node: check account ownership and make sure each transaction is analyzed once."""

import logging

from langgraph.graph import END

from agent.logging import NODE, log_route, traced_node
from agent.state import AnalyzerState
from db import claim_analysis, user_owns_account

_log = logging.getLogger(NODE)


@traced_node("claim")
def claim_transaction(state: AnalyzerState) -> dict:
    """Check the account belongs to the user, then claim the transaction so it runs once."""
    transaction, user_id = state["transaction"], state["user_id"]
    if not user_owns_account(user_id, transaction.account_id):
        return {"status": "FAILED", "error": "account_not_owned"}
    won, existing = claim_analysis(transaction.transaction_id, user_id)
    if not won:
        _log.info("claim.duplicate existing_status=%s", existing)
        return {"status": "DUPLICATE", "error": None}
    _log.info("claim.won")
    return {"status": "STARTED"}


def route_after_claim(state: AnalyzerState) -> str:
    """Continue to load_context when the claim was won; otherwise end the run."""
    status = state.get("status")
    if status == "STARTED":
        log_route("claim", "load_context", "claimed")
        return "load_context"
    log_route("claim", "END", "duplicate" if status == "DUPLICATE" else "account_not_owned")
    return END
