"""Integration tests: need Postgres running with the migrations applied."""

import uuid

import pytest

from db import (
    claim_analysis,
    finish_analysis,
    get_connection,
    load_rules,
    user_owns_account,
)


@pytest.fixture
def user_with_rules():
    """Create a user with two accounts and three rules; delete them after."""
    tag = uuid.uuid4().hex[:8]
    with get_connection() as conn:
        user_id = conn.execute(
            "INSERT INTO users (email, password_hash) VALUES (%s, 'x') RETURNING id",
            (f"test-{tag}@example.com",),
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES (%s, %s, 'tok')",
            (user_id, f"item-{tag}"),
        )
        for suffix in ("a", "b"):
            conn.execute(
                "INSERT INTO plaid_accounts (account_id, item_id, name, account_type) "
                "VALUES (%s, %s, %s, 'depository')",
                (f"acc-{suffix}-{tag}", f"item-{tag}", f"Account {suffix}"),
            )
        everywhere = conn.execute(
            "INSERT INTO rules (user_id, description) VALUES (%s, 'no gambling') RETURNING id",
            (user_id,),
        ).fetchone()[0]
        only_a = conn.execute(
            "INSERT INTO rules (user_id, description) VALUES (%s, 'no food on a') RETURNING id",
            (user_id,),
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO rule_accounts (rule_id, account_id) VALUES (%s, %s)",
            (only_a, f"acc-a-{tag}"),
        )
        conn.execute(
            "INSERT INTO rules (user_id, description, enabled) VALUES (%s, 'disabled', false)",
            (user_id,),
        )
    yield user_id, tag, everywhere, only_a
    with get_connection() as conn:
        conn.execute("DELETE FROM users WHERE id = %s", (user_id,))


def test_load_rules_filters_by_account_and_enabled(user_with_rules):
    """Rules for all accounts plus rules for this account; disabled rules are skipped."""
    user_id, tag, _, _ = user_with_rules

    on_a = load_rules(user_id, f"acc-a-{tag}")
    on_b = load_rules(user_id, f"acc-b-{tag}")

    assert sorted(r.description for r in on_a) == ["no food on a", "no gambling"]
    assert [r.description for r in on_b] == ["no gambling"]


def test_user_owns_account(user_with_rules):
    """Only the owning user owns the account."""
    user_id, tag, _, _ = user_with_rules

    assert user_owns_account(user_id, f"acc-a-{tag}") is True
    assert user_owns_account(user_id + 999_999, f"acc-a-{tag}") is False
    assert user_owns_account(user_id, "nope") is False


def test_claim_lifecycle(user_with_rules):
    """First claim wins, repeats lose, failed and stale claims can be re-claimed."""
    user_id, tag, _, _ = user_with_rules
    txn = f"txn-{tag}"

    assert claim_analysis(txn, user_id) == (True, None)
    assert claim_analysis(txn, user_id) == (False, "started")

    finish_analysis(txn, "failed", None, False, "boom", None, None)
    assert claim_analysis(txn, user_id) == (True, None)

    finish_analysis(txn, "done", "OK", False, None, False, None)
    assert claim_analysis(txn, user_id) == (False, "done")

    with get_connection() as conn:
        conn.execute(
            "UPDATE transaction_analyses SET status = 'started', "
            "updated_at = NOW() - INTERVAL '11 minutes' WHERE transaction_id = %s",
            (txn,),
        )
    assert claim_analysis(txn, user_id) == (True, None)


@pytest.mark.parametrize(
    ("flagged", "reason"),
    [(True, "$900.00 is far above your typical purchase"), (False, None), (None, None)],
)
def test_finish_analysis_records_the_anomaly(user_with_rules, flagged, reason):
    """The anomaly result is saved: flagged with a reason, checked and clean, or not run."""
    user_id, tag, _, _ = user_with_rules
    txn = f"txn-anomaly-{tag}-{flagged}"
    claim_analysis(txn, user_id)

    finish_analysis(txn, "done", "REVIEW", True, None, flagged, reason)

    with get_connection() as conn:
        row = conn.execute(
            "SELECT status, verdict, notified, anomaly_flagged, anomaly_reason "
            "FROM transaction_analyses WHERE transaction_id = %s",
            (txn,),
        ).fetchone()
    assert row == ("done", "REVIEW", True, flagged, reason)
