"""Aggregate queries over one user's transactions. Windows are half-open: [start, end)."""

from dataclasses import dataclass
from datetime import date

from db.connection import get_connection
from db.queries import _USER_ACCOUNTS


@dataclass(frozen=True)
class MerchantTotal:
    """Money spent at one merchant."""

    name: str
    total: float
    count: int


def category_totals(user_id: int, start: date, end: date) -> dict[str, float]:
    """Return money out (positive amounts) per category, biggest first."""
    with get_connection() as conn:
        rows = conn.execute(
            f"""
            SELECT COALESCE(t.category, 'OTHER'), SUM(t.amount)
            FROM plaid_transactions t
            JOIN {_USER_ACCOUNTS} ON a.account_id = t.account_id
            WHERE l.user_id = %s AND t.amount > 0 AND t.date >= %s AND t.date < %s
            GROUP BY 1
            ORDER BY 2 DESC, 1
            """,
            (user_id, start, end),
        ).fetchall()
    return {r[0]: float(r[1]) for r in rows}


def merchant_totals(user_id: int, start: date, end: date, limit: int) -> list[MerchantTotal]:
    """Return the merchants the user spent the most at, biggest first."""
    with get_connection() as conn:
        rows = conn.execute(
            f"""
            SELECT t.name, SUM(t.amount), COUNT(*)
            FROM plaid_transactions t
            JOIN {_USER_ACCOUNTS} ON a.account_id = t.account_id
            WHERE l.user_id = %s AND t.amount > 0 AND t.date >= %s AND t.date < %s
            GROUP BY t.name
            ORDER BY 2 DESC, 1
            LIMIT %s
            """,
            (user_id, start, end, limit),
        ).fetchall()
    return [MerchantTotal(name=r[0], total=float(r[1]), count=r[2]) for r in rows]


def net_flow(user_id: int, start: date, end: date) -> float:
    """Return deposits minus withdrawals on the user's checking and savings accounts."""
    with get_connection() as conn:
        row = conn.execute(
            f"""
            SELECT COALESCE(-SUM(t.amount), 0)
            FROM plaid_transactions t
            JOIN {_USER_ACCOUNTS} ON a.account_id = t.account_id
            WHERE l.user_id = %s AND a.account_type = 'depository'
              AND t.date >= %s AND t.date < %s
            """,
            (user_id, start, end),
        ).fetchone()
    return float(row[0])
