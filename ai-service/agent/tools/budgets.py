"""Tool: the user's monthly budgets."""

from datetime import date
from typing import Annotated

from langchain_core.tools import tool
from langgraph.prebuilt import InjectedState

from agent.safety import sanitize
from agent.tools.formatting import cap, money
from db import load_budgets


@tool
def get_budgets(state: Annotated[dict, InjectedState]) -> str:
    """List the user's active monthly budgets and how much is spent so far this month."""
    today = date.today()
    lines = [
        f"- {sanitize(b.name, 60)[0]} [{b.category}]: spent {money(b.spent_this_month)} "
        f"of {money(b.monthly_limit)}"
        for b in load_budgets(state["user_id"], today)
    ]
    return cap(f"Budgets for {today:%B %Y}:\n" + ("\n".join(lines) or "- none"))
