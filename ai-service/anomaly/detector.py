from statistics import mean, median
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


class ZScoreDetector:
    """
    Flags new purchases far above the user's usual spending, using the modified z-score.

    The modified z-score uses the median and median absolute deviation (MAD) instead of
    the mean and standard deviation, so a few big past purchases don't hide new ones.
    A transaction with a category is compared only to past purchases in that category,
    when there are enough of them; otherwise it's compared to every past purchase.
    Positive amounts are money going out (Plaid's convention).
    """

    def __init__(self, threshold: float = 3.5, min_history: int = 3):
        self.threshold = threshold
        self.min_history = min_history

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Flag new purchases whose modified z-score is above `threshold`."""
        purchases = [t for t in history if t.amount > 0]
        flagged = []
        for t in new:
            past, label = self._comparable_amounts(t, purchases)
            if t.amount <= 0 or len(past) < self.min_history:
                continue
            score = modified_z_score(t.amount, past)
            if score is not None and score > self.threshold:
                flagged.append(
                    FlaggedTransaction(
                        transaction=t,
                        reason=(
                            f"${t.amount:,.2f} is far above your typical {label} of "
                            f"${median(past):,.2f} (modified z-score {score:.1f})"
                        ),
                    )
                )
        return flagged

    def _comparable_amounts(
        self, t: Transaction, purchases: list[Transaction]
    ) -> tuple[list[float], str]:
        """Past amounts to compare against, and a label for them in the reason."""
        same_category = [p.amount for p in purchases if t.category and p.category == t.category]
        if len(same_category) >= self.min_history:
            return same_category, f"{t.category} purchase"
        return [p.amount for p in purchases], "purchase"


def modified_z_score(x: float, values: list[float]) -> float | None:
    """
    How unusual `x` is compared to `values`, or None if `values` are all identical.

    When over half the values are identical the MAD is 0, so fall back to the mean
    absolute deviation (scaled to match).
    """
    middle = median(values)
    deviations = [abs(v - middle) for v in values]
    mad = median(deviations)
    if mad:
        return 0.6745 * (x - middle) / mad
    mean_ad = mean(deviations)
    if mean_ad:
        return (x - middle) / (1.253314 * mean_ad)
    return None


class DuplicateDetector:
    """Flags new charges with the same name and amount as another charge a few days apart."""

    def __init__(self, window_days: int = 4):
        self.window_days = window_days

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Flag new charges that repeat a charge less than `window_days` days away."""
        seen = list(history)
        flagged = []
        for t in new:
            # Compare against history and earlier new charges, so only the second of a pair is flagged.
            match = next((s for s in seen if t.amount > 0 and self._is_duplicate(t, s)), None)
            if match:
                flagged.append(
                    FlaggedTransaction(
                        transaction=t,
                        reason=(
                            f"Possible duplicate: {t.name} also charged "
                            f"${t.amount:,.2f} on {match.date:%b %d}"
                        ),
                    )
                )
            seen.append(t)
        return flagged

    def _is_duplicate(self, a: Transaction, b: Transaction) -> bool:
        """Same name and amount, less than `window_days` days apart."""
        return (
            a.name == b.name
            and a.amount == b.amount
            and abs((a.date - b.date).days) < self.window_days
        )


class SubscriptionIncreaseDetector:
    """
    Flags a recurring charge that costs more than it used to.

    A charge counts as recurring when there are at least `min_previous` past charges
    with the same name, all for the same amount, at evenly spaced dates (give or take
    `tolerance_days`, since monthly bills land anywhere from 28 to 31 days apart).
    """

    def __init__(self, min_previous: int = 4, tolerance_days: int = 3):
        self.min_previous = min_previous
        self.tolerance_days = tolerance_days

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Flag new charges that are higher than a steady recurring price."""
        flagged = []
        for t in new:
            past = sorted(
                (h for h in history if h.name == t.name and h.amount > 0), key=lambda h: h.date
            )
            if len(past) < self.min_previous:
                continue
            price = past[0].amount
            if t.amount <= price or any(h.amount != price for h in past):
                continue
            gaps = [(b.date - a.date).days for a, b in zip(past, past[1:])]
            if min(gaps) <= 0 or max(gaps) - min(gaps) > self.tolerance_days:
                continue
            flagged.append(
                FlaggedTransaction(
                    transaction=t,
                    reason=f"{t.name} went up from ${price:,.2f} to ${t.amount:,.2f}",
                )
            )
        return flagged


class CombinedDetector:
    """Runs several detectors and reports each flagged transaction once, with every reason."""

    def __init__(self, detectors: list[AnomalyDetector]):
        self.detectors = detectors

    def find_anomalies(
        self, new: list[Transaction], history: list[Transaction]
    ) -> list[FlaggedTransaction]:
        """Flag what any detector flags, joining reasons when several flag the same transaction."""
        reasons: dict[str, list[str]] = {}
        for detector in self.detectors:
            for f in detector.find_anomalies(new, history):
                reasons.setdefault(f.transaction.transaction_id, []).append(f.reason)
        return [
            FlaggedTransaction(transaction=t, reason="; ".join(reasons[t.transaction_id]))
            for t in new
            if t.transaction_id in reasons
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
