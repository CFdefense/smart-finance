from statistics import median
from typing import Protocol

from monitor.data_models import FlaggedTransaction, Transaction


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
