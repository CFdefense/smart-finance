from langchain_core.language_models import BaseChatModel
from langchain_core.messages import SystemMessage

from agent.logging import traced_node
from agent.prompts import CHAT_SYSTEM_PROMPT
from agent.state import ChatState, StateUpdate


def make_respond(llm: BaseChatModel):
    """Build the node that answers the user's message using their data."""

    @traced_node("respond")
    def respond(state: ChatState) -> StateUpdate:
        """Generate a conversational response grounded in the user's data."""
        result = llm.invoke(
            [
                SystemMessage(content=CHAT_SYSTEM_PROMPT + state["context"]),
                *state["messages"],
            ]
        )
        return {"messages": result}

    return respond
