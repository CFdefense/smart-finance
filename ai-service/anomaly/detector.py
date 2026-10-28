from statistics import median
from typing import Protocol

from anomaly.data_models import AnomalyRequest, AnomalyResponse, FlaggedTransaction, Transaction
from db import get_connection


class AnomalyDetector(Protocol):
    """Anything that can pick out unusual transactions."""

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Return the new transactions that look unusual compared to history."""
        ...


class MedianDetector:
    """
    Placeholder detector: flags new purchases over 3x the user's typical purchase.

    Compares against ALL past purchases, because transactions don't have a
    category yet. Positive amounts are money going out (Plaid's convention).
    """

    def __init__(self, multiplier: float = 3.0, min_history: int = 3):
        self.multiplier = multiplier
        self.min_history = min_history

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Flag new purchases more than `multiplier` times the median past purchase."""
        past = [t.amount for t in history if t.amount > 0]
        if len(past) < self.min_history:
            return []

        typical = median(past)
        return [
            FlaggedTransaction(
                transaction=t,
                reason=(
                    f"${t.amount:,.2f} is over {self.multiplier:g}x "
                    f"your typical purchase of ${typical:,.2f}"
                ),
            )
            for t in new
            if t.amount > self.multiplier * typical
        ]


def load_transactions(user_id: int) -> list[Transaction]:
    """Load all of one user's transactions from the database."""
    with get_connection() as conn:
        rows = conn.execute(
            """
            SELECT t.transaction_id, t.name, t.amount, t.date
            FROM plaid_transactions t
            JOIN plaid_accounts a ON a.account_id = t.account_id
            JOIN plaid_links l ON l.item_id = a.item_id
            WHERE l.user_id = %s
            """,
            (user_id,),
        ).fetchall()
    return [Transaction(transaction_id=r[0], name=r[1], amount=r[2], date=r[3]) for r in rows]


def check_transactions(request: AnomalyRequest, detector: AnomalyDetector) -> AnomalyResponse:
    """Check a user's new transactions against their history and return the flagged ones."""
    everything = load_transactions(request.user_id)
    new = [t for t in everything if t.transaction_id in request.transaction_ids]
    history = [t for t in everything if t.transaction_id not in request.transaction_ids]
    flagged = detector.find_anomalies(new, history)
    return AnomalyResponse(flagged=flagged)
