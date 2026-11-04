from pydantic import BaseModel

from db import Transaction


class AnomalyRequest(BaseModel):
    """Which of a user's newly synced transactions to check."""

    user_id: int
    transaction_ids: list[str]


class FlaggedTransaction(BaseModel):
    """A transaction that looks unusual, and why."""

    transaction: Transaction
    reason: str


class AnomalyResponse(BaseModel):
    """The flagged transactions (empty if nothing looks unusual)."""

    flagged: list[FlaggedTransaction]
