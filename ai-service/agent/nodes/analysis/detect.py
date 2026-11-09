"""Anomaly node: check the transaction against the user's history with the rule-based detectors."""

import logging

from agent.logging import NODE, traced_node
from agent.state import AnalyzerState, StateUpdate
from anomaly import AnomalyDetector
from db import Transaction, load_transactions

_log = logging.getLogger(NODE)


def make_detect_anomaly(detector: AnomalyDetector):
    """Build the node that flags the transaction if it looks unusual compared to history."""

    @traced_node("anomaly")
    def detect_anomaly(state: AnalyzerState) -> StateUpdate:
        """
        Write `anomaly_flagged` and `anomaly_reason`; both are None if detection failed.

        The transaction's own stored row is left out of history so it is never compared to
        itself. The transaction is already claimed, so a failure here must not abort the run.
        """
        txn = state["transaction"]
        current = Transaction(
            transaction_id=txn.transaction_id,
            name=txn.name,
            amount=txn.amount,
            date=txn.date,
            category=txn.category,
        )
        try:
            history = [
                t
                for t in load_transactions(state["user_id"])
                if t.transaction_id != txn.transaction_id
            ]
            flagged = detector.find_anomalies([current], history)
        except Exception as exc:
            _log.warning("anomaly.failed continuing without detection type=%s", type(exc).__name__)
            _log.debug("anomaly.failed detail", exc_info=True)
            return {"anomaly_flagged": None, "anomaly_reason": None}
        if not flagged:
            return {"anomaly_flagged": False, "anomaly_reason": None}
        return {"anomaly_flagged": True, "anomaly_reason": flagged[0].reason}

    return detect_anomaly
