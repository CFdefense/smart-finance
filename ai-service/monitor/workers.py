from concurrent.futures import ThreadPoolExecutor

from db import get_connection
from monitor.data_models import FlaggedTransaction, Transaction
from monitor.detector import AnomalyDetector, MedianDetector

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


# The job queue. Each request is queued here as a job; at most 4 run at once and
# the rest wait in line, so a burst of requests can't overload the database.
job_queue = ThreadPoolExecutor(max_workers=4, thread_name_prefix="anomaly-worker")


def submit_check(user_id: int, new_ids: list[str]) -> list[FlaggedTransaction]:
    """Queue an anomaly check, wait for a worker to finish it, and return the result."""
    future = job_queue.submit(check_transactions, user_id, new_ids)
    return future.result(timeout=30)
