from typing import Annotated

from langgraph.graph.message import add_messages
from typing_extensions import TypedDict

from agent.data_models import AnalyzedTransaction, Assessment


class ChatState(TypedDict):
    """Represent the state of a chat conversation with message history to handle tool calls."""

    user_id: int
    context: str
    messages: Annotated[list, add_messages]


class AnalyzerState(TypedDict):
    """State for the per-transaction analyzer graph."""

    run_id: str  # correlation id for logs
    user_id: int  # set once from the request
    transaction: AnalyzedTransaction  # frozen; no node ever returns this key
    context: str  # accounts summary
    messages: Annotated[list, add_messages]  # investigate <-> tools loop
    assessment: Assessment | None
    notify: bool  # decide's output
    notified: bool
    status: str
    error: str | None
