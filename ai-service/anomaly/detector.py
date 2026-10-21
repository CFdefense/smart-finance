from statistics import median
from typing import Protocol

from db import get_connection
from anomaly.data_models import FlaggedTransaction, Transaction


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


# The detector the service uses. To switch to a better one later, change only this line.
detector: AnomalyDetector = MedianDetector()


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


def check_transactions(user_id: int, new_ids: list[str]) -> list[FlaggedTransaction]:
    """Check a user's new transactions against their history and return the flagged ones."""
    everything = load_transactions(user_id)
    new = [t for t in everything if t.transaction_id in new_ids]
    history = [t for t in everything if t.transaction_id not in new_ids]
    return detector.find_anomalies(new, history)

