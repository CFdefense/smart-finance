"""Tool: the user's savings goals."""

from typing import Annotated

from langchain_core.tools import tool
from langgraph.prebuilt import InjectedState

from agent.formatting import money
from agent.safety import sanitize
from agent.tools.formatting import cap
from db import load_goals


@tool
def get_goals(state: Annotated[dict, InjectedState]) -> str:
    """List the user's savings goals with current balance, starting balance and target."""
    lines = [
        f"- {sanitize(g.name, 60)[0]} on {sanitize(g.account_name, 60)[0]}: "
        f"balance {money(g.current_balance)}, started at {money(g.baseline_balance)}, "
        f"target {money(g.target_amount)}"
        + (f" by {g.target_date.isoformat()}" if g.target_date else "")
        for g in load_goals(state["user_id"])
    ]
    return cap("Goals:\n" + ("\n".join(lines) or "- none"))
