"""Graph nodes for the scope guard: decide, route, and refuse."""

import logging

from langchain_core.messages import AIMessage

from agent.logging import NODE, log_route, traced_node
from agent.scope import ScopeGuard, refusal_for
from agent.state import ChatState, StateUpdate

_log = logging.getLogger(NODE)


def make_scope_guard(guard: ScopeGuard):
    """Build the node that asks `guard` whether the latest message is in scope."""

    @traced_node("scope_guard")
    def scope_guard(state: ChatState) -> StateUpdate:
        """Record the guard's decision. A guard failure propagates, so the chat fails closed."""
        decision = guard.check(state["scope_message"], state["scope_history"])
        _log.info(
            "scope.verdict layer=%s category=%s allowed=%s",
            decision.layer,
            decision.category,
            decision.allowed,
        )
        return {"scope_decision": decision}

    return scope_guard


def route_after_scope(state: ChatState) -> str:
    """Continue to the chat for an allowed message; otherwise refuse."""
    decision = state["scope_decision"]
    target = "load_context" if decision.allowed else "refuse"
    log_route("scope_guard", target, f"category={decision.category}")
    return target


@traced_node("refuse")
def refuse(state: ChatState) -> StateUpdate:
    """Answer with the fixed refusal for the decided category."""
    return {"messages": AIMessage(content=refusal_for(state["scope_decision"].category))}
