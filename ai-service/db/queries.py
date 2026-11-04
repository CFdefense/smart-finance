"""Functions that load one user's data from the database."""

from datetime import date

from db.connection import get_connection
from db.models import Account, Budget, Goal, NotificationPreferences, Rule, Transaction

# Joins from a user to everything linked through their Plaid items.
_USER_ACCOUNTS = """
    plaid_accounts a
    JOIN plaid_links l ON l.item_id = a.item_id
"""


def load_transactions(
    user_id: int, since: date | None = None, limit: int | None = None
) -> list[Transaction]:
    """Load one user's transactions, newest first, optionally only those on or after `since`."""
    with get_connection() as conn:
        rows = conn.execute(
            f"""
            SELECT t.transaction_id, t.name, t.amount, t.date, t.category
            FROM plaid_transactions t
            JOIN {_USER_ACCOUNTS} ON a.account_id = t.account_id
            WHERE l.user_id = %s
              AND (%s::date IS NULL OR t.date >= %s::date)
            ORDER BY t.date DESC, t.id DESC
            LIMIT %s
            """,
            (user_id, since, since, limit),
        ).fetchall()
    return [
        Transaction(transaction_id=r[0], name=r[1], amount=r[2], date=r[3], category=r[4])
        for r in rows
    ]


def load_accounts(user_id: int) -> list[Account]:
    """Load one user's linked accounts."""
    with get_connection() as conn:
        rows = conn.execute(
            f"""
            SELECT a.account_id, a.name, a.account_type, a.account_subtype,
                   a.current_balance, a.available_balance
            FROM {_USER_ACCOUNTS}
            WHERE l.user_id = %s
            ORDER BY a.name
            """,
            (user_id,),
        ).fetchall()
    return [
        Account(
            account_id=r[0],
            name=r[1],
            account_type=r[2],
            account_subtype=r[3],
            current_balance=r[4],
            available_balance=r[5],
        )
        for r in rows
    ]


def load_budgets(user_id: int, today: date) -> list[Budget]:
    """
    Load one user's active budgets with spending for the month containing `today`.

    Spending is money out (positive amounts) in the budget's category, matching the backend.
    """
    month_start = today.replace(day=1)
    with get_connection() as conn:
        rows = conn.execute(
            f"""
            SELECT b.name, b.category, b.monthly_limit,
                   COALESCE((
                       SELECT SUM(t.amount)
                       FROM plaid_transactions t
                       JOIN {_USER_ACCOUNTS} ON a.account_id = t.account_id
                       WHERE l.user_id = b.user_id
                         AND t.category = b.category
                         AND t.amount > 0
                         AND t.date >= %s
                         AND t.date < (%s::date + INTERVAL '1 month')
                   ), 0)
            FROM budgets b
            WHERE b.user_id = %s AND b.active
            ORDER BY b.name
            """,
            (month_start, month_start, user_id),
        ).fetchall()
    return [
        Budget(name=r[0], category=r[1], monthly_limit=r[2], spent_this_month=r[3]) for r in rows
    ]


def load_goals(user_id: int) -> list[Goal]:
    """Load one user's goals that haven't been archived."""
    with get_connection() as conn:
        rows = conn.execute(
            """
            SELECT g.name, g.description, a.name, g.target_amount, g.baseline_balance,
                   a.current_balance, g.target_date
            FROM goals g
            JOIN plaid_accounts a ON a.account_id = g.account_id
            WHERE g.user_id = %s AND NOT g.archived
            ORDER BY g.name
            """,
            (user_id,),
        ).fetchall()
    return [
        Goal(
            name=r[0],
            description=r[1],
            account_name=r[2],
            target_amount=r[3],
            baseline_balance=r[4],
            current_balance=r[5],
            target_date=r[6],
        )
        for r in rows
    ]


def load_preferences(user_id: int) -> NotificationPreferences:
    """Load one user's notification preferences. Raises ValueError for an unknown user."""
    with get_connection() as conn:
        row = conn.execute(
            """
            SELECT email, email_notifications, phone_number, sms_notifications,
                   sms_carrier_gateway
            FROM users
            WHERE id = %s
            """,
            (user_id,),
        ).fetchone()

    if row is None:
        raise ValueError(f"No user found with id={user_id}")

    return NotificationPreferences(
        email=row[0],
        email_notifications=row[1],
        phone_number=row[2],
        sms_notifications=row[3],
        sms_carrier_gateway=row[4],
    )


def load_rules(user_id: int, account_id: str) -> list[Rule]:
    """Load the user's enabled rules that apply to `account_id` (no linked accounts = all)."""
    with get_connection() as conn:
        rows = conn.execute(
            """
            SELECT r.id, r.description,
                   COALESCE(ARRAY(
                       SELECT ra.account_id FROM rule_accounts ra
                       WHERE ra.rule_id = r.id ORDER BY ra.account_id
                   ), ARRAY[]::text[])
            FROM rules r
            WHERE r.user_id = %s AND r.enabled
              AND (
                  NOT EXISTS (SELECT 1 FROM rule_accounts ra WHERE ra.rule_id = r.id)
                  OR EXISTS (
                      SELECT 1 FROM rule_accounts ra
                      WHERE ra.rule_id = r.id AND ra.account_id = %s
                  )
              )
            ORDER BY r.id
            """,
            (user_id, account_id),
        ).fetchall()
    return [Rule(id=r[0], description=r[1], account_ids=list(r[2])) for r in rows]


def user_owns_account(user_id: int, account_id: str) -> bool:
    """Return whether `account_id` is linked to `user_id`."""
    with get_connection() as conn:
        row = conn.execute(
            f"SELECT 1 FROM {_USER_ACCOUNTS} WHERE l.user_id = %s AND a.account_id = %s",
            (user_id, account_id),
        ).fetchone()
    return row is not None
