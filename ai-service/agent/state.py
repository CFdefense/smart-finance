from typing_extensions import TypedDict


class AgentState(TypedDict):
    """Represent the input and output state of an agent."""

    user_id: int
    message: str
    context: str
    response: str
