"""Finalization node recording the transaction analysis outcome."""

import logging

from agent.logging import NODE, traced_node
from agent.state import AnalyzerState
from db import finish_analysis

_log = logging.getLogger(NODE)


@traced_node("finalize")
def finalize(state: AnalyzerState) -> dict:
    """Record the outcome so the claim reflects it; failed runs may be claimed again."""
    failed = state.get("status") == "FAILED"
    assessment = state.get("assessment")
    for attempt in (1, 2):
        try:
            finish_analysis(
                state["transaction"].transaction_id,
                "failed" if failed else "done",
                assessment.verdict if assessment else None,
                bool(state.get("notified")),
                state.get("error"),
            )
            break
        except Exception as exc:
            if attempt == 1:
                _log.warning("finalize.retry type=%s", type(exc).__name__)
                _log.debug("finalize.retry detail", exc_info=True)
            else:
                _log.error("finalize.record_failed could not save the outcome", exc_info=True)
    return {"status": "FAILED" if failed else "ANALYZED"}
