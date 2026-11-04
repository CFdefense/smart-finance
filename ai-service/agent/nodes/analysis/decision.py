"""Decision node and routing after assessment."""

import logging

from agent.logging import DECISION, log_route, traced_node
from agent.state import AnalyzerState

_log = logging.getLogger(DECISION)


def decide_notification(verdict: str, should_notify: bool) -> tuple[bool, str]:
    """Apply the firm policy; returns whether to notify and the name of the rule applied."""
    if verdict == "OK":
        return False, "ok_never_notifies"
    if verdict == "ALERT":
        return True, "alert_always_notifies"
    return should_notify, "review_follows_model"


@traced_node("decide")
def decide(state: AnalyzerState) -> dict:
    """Combine the model's assessment with the policy to decide whether to notify."""
    assessment = state["assessment"]
    notify, rule = decide_notification(assessment.verdict, assessment.should_notify)
    _log.info(
        "decision verdict=%s model_should_notify=%s -> notify=%s rule=%s",
        assessment.verdict,
        assessment.should_notify,
        notify,
        rule,
    )
    return {"notify": notify}


def route_after_decide(state: AnalyzerState) -> str:
    """Send the user a notification when decided; otherwise go straight to finalize."""
    if state.get("notify"):
        log_route("decide", "notify", "notify=true")
        return "notify"
    log_route("decide", "finalize", "notify=false")
    return "finalize"
