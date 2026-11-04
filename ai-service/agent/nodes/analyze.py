from collections.abc import Callable

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import HumanMessage, SystemMessage

from agent.prompts import ANALYZE_SYSTEM_PROMPT
from agent.state import AgentState


def make_analyze(llm: BaseChatModel) -> Callable[[AgentState], dict]:
    """Build the node that evaluates a transaction-related request."""

    def analyze(state: AgentState) -> dict:
        """Evaluate a transaction-related request."""
        result = llm.invoke(
            [
                SystemMessage(content=ANALYZE_SYSTEM_PROMPT),
                HumanMessage(content=state["message"]),
            ]
        )
        return {"response": str(result.content)}

    return analyze
