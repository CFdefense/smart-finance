from collections.abc import Callable

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import HumanMessage, SystemMessage

from agent.prompts import CHAT_SYSTEM_PROMPT
from agent.state import AgentState


def make_respond(llm: BaseChatModel) -> Callable[[AgentState], dict]:
    """Build the node that answers the user's message using their data."""

    def respond(state: AgentState) -> dict:
        """Generate a conversational response grounded in the user's data."""
        result = llm.invoke(
            [
                SystemMessage(content=CHAT_SYSTEM_PROMPT + state["context"]),
                HumanMessage(content=state["message"]),
            ]
        )
        return {"response": str(result.content)}

    return respond
