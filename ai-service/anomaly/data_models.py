from datetime import date

from pydantic import BaseModel


class AnomalyRequest(BaseModel):
    """Which of a user's newly synced transactions to check."""

    user_id: int
    transaction_ids: list[str]


class Transaction(BaseModel):
    """One transaction loaded from the database."""

    transaction_id: str
    name: str
    amount: float
    date: date


class FlaggedTransaction(BaseModel):
    """A transaction that looks unusual, and why."""

    transaction: Transaction
    reason: str


class AnomalyResponse(BaseModel):
    """The flagged transactions (empty if nothing looks unusual)."""

    flagged: list[FlaggedTransaction]
