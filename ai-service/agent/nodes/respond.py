from collections.abc import Callable

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import SystemMessage

from agent.prompts import CHAT_SYSTEM_PROMPT
from agent.state import ChatState


def make_respond(llm: BaseChatModel) -> Callable[[ChatState], dict]:
    """Build the node that answers the user's message using their data."""

    def respond(state: ChatState) -> dict:
        """Generate a conversational response grounded in the user's data."""
        result = llm.invoke(
            [
                SystemMessage(content=CHAT_SYSTEM_PROMPT + state["context"]),
                *state["messages"],
            ]
        )
        return {"messages": result}

    return respond
