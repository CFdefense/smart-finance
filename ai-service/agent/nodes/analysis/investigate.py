"""Investigate node and routing for gathering additional transaction context."""

import logging
from typing import Any

from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, ToolMessage

from agent.logging import GRAPH, NODE, log_route, traced_node
from agent.prompts import INVESTIGATE_SYSTEM_PROMPT, transaction_block
from agent.state import AnalyzerState, StateUpdate

MAX_TOOL_ROUNDS = 4
_MAX_FINDINGS_CHARS = 8000
_log = logging.getLogger(NODE)


def _rounds(messages: list[Any]) -> int:
    """Count the model replies that asked for tools."""
    return sum(1 for m in messages if isinstance(m, AIMessage) and m.tool_calls)


def make_investigate(llm_with_tools):
    """Build the node where the model may call tools to gather more information."""

    @traced_node("investigate")
    def investigate(state: AnalyzerState) -> StateUpdate:
        """Ask the model what else it needs; seed the conversation on the first call."""
        history, seed = state["messages"], []
        if not history:
            seed = [
                HumanMessage(
                    content=f"{transaction_block(state['transaction'])}\n\n{state['context']}"
                )
            ]
            history = seed
        try:
            reply = llm_with_tools.invoke(
                [SystemMessage(content=INVESTIGATE_SYSTEM_PROMPT), *history]
            )
        except Exception as exc:
            _log.warning(
                "investigate.llm_failed continuing without more data type=%s", type(exc).__name__
            )
            _log.debug("investigate.llm_failed detail", exc_info=True)
            reply = AIMessage(content="Investigation unavailable.")
        return {"messages": [*seed, reply]}

    return investigate


def route_after_investigate(state: AnalyzerState) -> str:
    """Go to the tools while the model asks for them and the round cap allows; else assess."""
    messages = state["messages"]
    last = messages[-1] if messages else None
    if isinstance(last, AIMessage) and last.tool_calls:
        if _rounds(messages) <= MAX_TOOL_ROUNDS:
            logging.getLogger(GRAPH).info(
                "route investigate -> tools tool_calls=%d round=%d",
                len(last.tool_calls),
                _rounds(messages),
            )
            return "tools"
        log_route("investigate", "assess", "round_cap")
        return "assess"
    log_route("investigate", "assess", "no_tool_calls")
    return "assess"


def build_findings(messages: list[Any]) -> str:
    """Turn tool results and the analyst's last note into plain text for the assess step."""
    parts = [f"[{m.name}]\n{m.content}" for m in messages if isinstance(m, ToolMessage)]
    notes = [m for m in messages if isinstance(m, AIMessage) and not m.tool_calls and m.content]
    if notes:
        parts.append(f"Analyst note: {notes[-1].content}")
    return "\n\n".join(parts)[:_MAX_FINDINGS_CHARS] or "(no extra data was gathered)"
