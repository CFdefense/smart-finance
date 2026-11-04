"""Assess node and routing for structured transaction assessment."""

import logging

from langchain_core.messages import HumanMessage, SystemMessage

from agent.data_models import Assessment
from agent.logging import NODE, log_route, traced_node
from agent.nodes.analysis.investigate import build_findings
from agent.prompts import ASSESS_SYSTEM_PROMPT, transaction_block
from agent.state import AnalyzerState, StateUpdate

_log = logging.getLogger(NODE)


def make_assess(structured_llm):
    """Build the node that turns everything gathered into a structured Assessment."""

    @traced_node("assess")
    def assess(state: AnalyzerState) -> StateUpdate:
        """Ask the model for an Assessment; retry once, then fail safe."""
        messages = [
            SystemMessage(content=ASSESS_SYSTEM_PROMPT),
            HumanMessage(
                content=(
                    f"{transaction_block(state['transaction'])}\n\n{state['context']}\n\n"
                    f"Findings:\n{build_findings(state['messages'])}"
                )
            ),
        ]
        for attempt in (1, 2):
            try:
                result = structured_llm.invoke(messages)
                if not isinstance(result, Assessment):
                    raise TypeError("model did not return an Assessment")
                return {"assessment": result}
            except Exception as exc:
                if attempt == 1:
                    _log.warning("assess.retry first attempt failed type=%s", type(exc).__name__)
                    _log.debug("assess.retry detail", exc_info=True)
                else:
                    _log.error(
                        "assess.failed giving up; no notification will be sent", exc_info=True
                    )
        return {"assessment": None, "status": "FAILED", "error": "assess_failed"}

    return assess


def route_after_assess(state: AnalyzerState) -> str:
    """Go to decide with an assessment; otherwise straight to finalize."""
    if state.get("assessment") is None:
        log_route("assess", "finalize", "no_assessment")
        return "finalize"
    log_route("assess", "decide", "assessed")
    return "decide"
