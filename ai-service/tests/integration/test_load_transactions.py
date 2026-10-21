"""Integration tests: these need Postgres running with the migrations applied."""

import uuid

import pytest

from db import get_connection
from monitor.workers import load_transactions


@pytest.fixture
def user_with_transactions():
    """Create a user with one bank account and four purchases; delete them afterwards."""
    tag = uuid.uuid4().hex[:8]  # unique per test run, so runs never collide
    with get_connection() as conn:
        user_id = conn.execute(
            "INSERT INTO users (email, password_hash) VALUES (%s, 'x') RETURNING id",
            (f"test-{tag}@example.com",),
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES (%s, %s, 'tok')",
            (user_id, f"item-{tag}"),
        )
        conn.execute(
            "INSERT INTO plaid_accounts (account_id, item_id, name, account_type) "
            "VALUES (%s, %s, 'Checking', 'depository')",
            (f"acc-{tag}", f"item-{tag}"),
        )
        for name, amount in [("coffee", 5), ("lunch", 15), ("groceries", 60), ("tv", 900)]:
            conn.execute(
                "INSERT INTO plaid_transactions (transaction_id, account_id, name, amount, date) "
                "VALUES (%s, %s, %s, %s, '2026-10-01')",
                (f"{name}-{tag}", f"acc-{tag}", name, amount),
            )

    yield user_id, tag

    # Deleting the user also deletes their links, accounts and transactions (ON DELETE CASCADE).
    with get_connection() as conn:
        conn.execute("DELETE FROM users WHERE id = %s", (user_id,))


def test_load_transactions_returns_the_users_transactions(user_with_transactions):
    """All four of the user's transactions come back with the right fields."""
    user_id, tag = user_with_transactions

    transactions = load_transactions(user_id)

    assert sorted(t.transaction_id for t in transactions) == sorted(
        f"{name}-{tag}" for name in ["coffee", "lunch", "groceries", "tv"]
    )
    tv = next(t for t in transactions if t.name == "tv")
    assert tv.amount == 900


def test_load_transactions_never_returns_other_users_transactions(user_with_transactions):
    """A different user ID gets none of this user's transactions."""
    user_id, _ = user_with_transactions

    assert load_transactions(user_id + 999_999) == []
