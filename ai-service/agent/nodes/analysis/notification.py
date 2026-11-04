"""Notification node for transaction analysis."""

import logging

from agent.logging import NODE, traced_node
from agent.safety import build_notification
from agent.state import AnalyzerState
from notifications import send_notification

_log = logging.getLogger(NODE)


@traced_node("notify")
def notify(state: AnalyzerState) -> dict:
    """Send the templated notification to the state's user. The model never picks the target."""
    txn, assessment = state["transaction"], state["assessment"]
    message = build_notification(txn.name, txn.amount, assessment.notify_reason)
    try:
        send_notification(state["user_id"], message)
    except Exception:
        _log.error("notify.failed could not deliver notification", exc_info=True)
        return {"notified": False, "status": "FAILED", "error": "notify_failed"}
    _log.debug("notify.message %s", message)
    return {"notified": True}
