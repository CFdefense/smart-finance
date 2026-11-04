"""Claim and finish transaction analyses so each transaction is analyzed once."""

from db.connection import get_connection


def claim_analysis(transaction_id: str, user_id: int) -> tuple[bool, str | None]:
    """
    Try to claim a transaction for analysis.

    Returns (True, None) when this caller won the claim. Returns (False, status) when it
    already exists, where status is the existing row's status. Failed rows and 'started'
    rows older than 10 minutes (a crashed run) can be claimed again.
    """
    with get_connection() as conn:
        won = conn.execute(
            """
            INSERT INTO transaction_analyses (transaction_id, user_id, status)
            VALUES (%s, %s, 'started')
            ON CONFLICT (transaction_id) DO UPDATE
                SET status = 'started', user_id = EXCLUDED.user_id, error = NULL,
                    updated_at = NOW()
                WHERE transaction_analyses.status = 'failed'
                   OR (transaction_analyses.status = 'started'
                       AND transaction_analyses.updated_at < NOW() - INTERVAL '10 minutes')
            RETURNING status
            """,
            (transaction_id, user_id),
        ).fetchone()
        if won is not None:
            return True, None
        existing = conn.execute(
            "SELECT status FROM transaction_analyses WHERE transaction_id = %s",
            (transaction_id,),
        ).fetchone()
    return False, existing[0] if existing else None


def finish_analysis(
    transaction_id: str, status: str, verdict: str | None, notified: bool, error: str | None
) -> None:
    """
    Record the outcome of a claimed analysis; `status` is 'done' or 'failed'.

    Only rows still in 'started' are updated, so a late or crashed run cannot overwrite a
    finished one.
    """
    with get_connection() as conn:
        conn.execute(
            """
            UPDATE transaction_analyses
            SET status = %s, verdict = %s, notified = %s, error = %s, updated_at = NOW()
            WHERE transaction_id = %s AND status = 'started'
            """,
            (status, verdict, notified, error, transaction_id),
        )
