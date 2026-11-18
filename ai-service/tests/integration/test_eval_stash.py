"""Integration test: the eval reset sets a persona's own transactions aside, then restores them."""

import uuid
from pathlib import Path

import psycopg
import pytest

import evaldb
from definition import parse

PERSONA = {"email": "stash@test.com", "first_name": "Stash"}
ACCOUNT = "acc-stash"


def _case(**test_vars):
    """Build an EvalCase for the test persona."""
    return parse(
        {"persona": PERSONA["email"], "agent": "chat", **test_vars}, {PERSONA["email"]: PERSONA}
    )


@pytest.fixture
def eval_db():
    """Create a throwaway eval database with the migrations and one user with one account."""
    admin = evaldb.admin_url()
    name = f"{evaldb.PREFIX}it_{uuid.uuid4().hex[:8]}"
    url = evaldb.database_url(admin, name)
    evaldb._create(admin, name)
    try:
        with psycopg.connect(url) as conn:
            for migration in sorted(Path(evaldb.MIGRATIONS).glob("*.sql")):
                conn.execute(migration.read_text())
            user_id = conn.execute(
                "INSERT INTO users (email, password_hash) VALUES (%s, 'x') RETURNING id",
                (PERSONA["email"],),
            ).fetchone()[0]
            conn.execute(
                "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES (%s, 'item', 't')",
                (user_id,),
            )
            conn.execute(
                "INSERT INTO plaid_accounts (account_id, item_id, name, account_type, "
                "current_balance) VALUES (%s, 'item', 'Checking', 'depository', 100)",
                (ACCOUNT,),
            )
            conn.execute(
                "INSERT INTO plaid_transactions (transaction_id, account_id, name, amount, date) "
                "VALUES ('plaid-1', %s, 'Real A', 5, now()), ('plaid-2', %s, 'Real B', 6, now())",
                (ACCOUNT, ACCOUNT),
            )
        yield url, user_id
    finally:
        evaldb._drop(admin, name)


def _names(url):
    """Return the transaction names currently visible to the agent, sorted."""
    with psycopg.connect(url) as conn:
        return sorted(r[0] for r in conn.execute("SELECT name FROM plaid_transactions"))


def test_seeded_cases_see_only_their_rows_and_the_next_reset_restores_the_rest(eval_db):
    """A seeded test is judged on exact totals; the persona's own rows come back afterwards."""
    url, user_id = eval_db
    accounts = {"Checking": ACCOUNT}
    seeded = _case(
        seed_transactions=[{"name": "Seeded", "amount": 10, "account": "Checking"}], message="hi"
    )
    plain = _case(message="hi")

    evaldb.reset(url, user_id, seeded, accounts)
    assert _names(url) == ["Seeded"]

    evaldb.reset(url, user_id, seeded, accounts)  # a second seeded test must not lose anything
    assert _names(url) == ["Seeded"]

    evaldb.reset(url, user_id, plain, accounts)
    assert _names(url) == ["Real A", "Real B"]

    evaldb.reset(url, user_id, plain, accounts)  # restoring twice is harmless
    assert _names(url) == ["Real A", "Real B"]


def test_a_failed_reset_loses_nothing(eval_db):
    """An unknown account in a seed aborts the whole reset, so no transaction is lost."""
    url, user_id = eval_db
    bad = _case(
        seed_transactions=[{"name": "Seeded", "amount": 10, "account": "Nope"}], message="hi"
    )

    with pytest.raises(Exception, match="Nope"):
        evaldb.reset(url, user_id, bad, {"Checking": ACCOUNT})

    assert _names(url) == ["Real A", "Real B"]
