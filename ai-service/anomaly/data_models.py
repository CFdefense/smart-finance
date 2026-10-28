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
    # Not loaded from the database yet, so always None for now. Detectors that use it
    # must fall back to the whole history when it's missing.
    category: str | None = None


class FlaggedTransaction(BaseModel):
    """A transaction that looks unusual, and why."""

    transaction: Transaction
    reason: str


class AnomalyResponse(BaseModel):
    """The flagged transactions (empty if nothing looks unusual)."""

    flagged: list[FlaggedTransaction]
