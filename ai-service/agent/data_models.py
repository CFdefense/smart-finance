from pydantic import BaseModel
from typing_extensions import TypedDict


class ChatRequest(BaseModel):
    """Represent an incoming chat request."""

    user_id: int
    message: str


class ChatResponse(BaseModel):
    """Represent a response returned by the AI service."""

    response: str


class Conversation(BaseModel):
    """Represent a user conversation."""

    user_id: int
    history: list[tuple[ChatRequest, ChatResponse]]


class AgentState(TypedDict):
    """Represent the input and output state of an agent."""

    user_id: int
    message: str
    response: str
