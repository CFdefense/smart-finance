from typing import Annotated

from langgraph.graph.message import add_messages
from typing_extensions import TypedDict


class AgentState(TypedDict):
    """Represent the input and output state of an agent."""

    user_id: int
    message: str
    context: str
    response: str

class ChatState(TypedDict):
    """Represent the state of a chat conversation with message history to handle tool calls."""
    
    user_id: int
    context: str
    messages: Annotated[list, add_messages]
