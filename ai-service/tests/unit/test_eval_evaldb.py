"""Tests for the eval database lifecycle, with psycopg and subprocess faked."""

from datetime import date, timedelta
from pathlib import Path
from types import SimpleNamespace

import psycopg
import pytest

import evaldb
from definition import DefinitionError, parse

PERSONA = {"email": "christian@test.com", "first_name": "Christian"}
PERSONAS = {PERSONA["email"]: PERSONA}
ADMIN = "postgres://postgres:postgres@localhost:5432/smart_finance"
EVAL_URL = "postgres://x/smartfinance_eval_ab_x"
ACCOUNTS = {"TD Bank Student Checking": "acc_1", "Discover it Student": "acc_2"}


class FakeCursor:
    """Returns canned rows."""

    def __init__(self, rows):
        self._rows = rows

    def fetchone(self):
        """Return the first canned row, or None."""
        return self._rows[0] if self._rows else None

    def fetchall(self):
        """Return all canned rows."""
        return self._rows


class FakeConn:
    """Records executed SQL and answers from a substring -> rows map."""

    def __init__(self):
        self.calls = []
        self.answers = {}

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def execute(self, query, params=None):
        """Record the normalized SQL text and params; answer from `answers`."""
        text = query.as_string(None) if hasattr(query, "as_string") else query
        text = " ".join(text.split())
        self.calls.append((text, params))
        for needle, rows in self.answers.items():
            if needle in text:
                return FakeCursor(rows)
        return FakeCursor([])


@pytest.fixture
def conn(monkeypatch):
    """Patch psycopg.connect to hand out one shared FakeConn."""
    fake = FakeConn()
    monkeypatch.setattr(evaldb.psycopg, "connect", lambda *a, **k: fake)
    return fake


def make_case(**overrides):
    """Build an EvalCase with a rule, budget and goal."""
    test_vars = {
        "persona": PERSONA["email"],
        "agent": "chat",
        "message": "hi",
        "budgets": [{"name": "Eating out", "category": "FOOD_DINING", "monthly_limit": 100}],
        "rules": [
            {"description": "No gambling."},
            {"description": "Flag over $200.", "account": "TD Bank Student Checking"},
        ],
        "goals": [
            {
                "name": "Trip",
                "description": "Save",
                "account": "TD Bank Student Checking",
                "target_amount": 2500,
                "days_until_target": 120,
            }
        ],
    }
    test_vars.update(overrides)
    return parse(test_vars, PERSONAS)


def test_db_name_is_prefixed_and_stable_within_a_run():
    """The same run and persona give the same name; another run gives another."""
    name = evaldb.db_name("eval-1", "christian@test.com")

    assert name.startswith(evaldb.PREFIX) and name.endswith("_christian")
    assert name == evaldb.db_name("eval-1", "christian@test.com")
    assert name != evaldb.db_name("eval-2", "christian@test.com")
    assert name.startswith(evaldb.PREFIX + evaldb.run_key("eval-1") + "_")


def test_database_url_swaps_only_the_database():
    """The credentials and host are kept; only the path changes."""
    assert (
        evaldb.database_url(ADMIN, "smartfinance_eval_ab_x")
        == "postgres://postgres:postgres@localhost:5432/smartfinance_eval_ab_x"
    )


def test_database_url_rejects_a_dbname_override_to_the_real_database():
    """A `dbname` query parameter would beat the path, so the real database is refused."""
    url = ADMIN + "?dbname=smart_finance&sslmode=require"

    with pytest.raises(ValueError, match="dbname") as err:
        evaldb.database_url(url, "smartfinance_eval_ab_x")
    assert "postgres:postgres" not in str(err.value)


def test_database_url_allows_a_dbname_override_equal_to_the_eval_name():
    """A `dbname` that already names the eval database is harmless."""
    url = ADMIN + "?dbname=smartfinance_eval_ab_x&sslmode=require"

    out = evaldb.database_url(url, "smartfinance_eval_ab_x")

    assert out.endswith("/smartfinance_eval_ab_x?dbname=smartfinance_eval_ab_x&sslmode=require")


def test_create_and_drop_quote_the_name(conn):
    """CREATE/DROP use a quoted identifier."""
    evaldb._create(ADMIN, "smartfinance_eval_ab_x")
    evaldb._drop(ADMIN, "smartfinance_eval_ab_x")

    assert conn.calls[0][0] == 'CREATE DATABASE "smartfinance_eval_ab_x"'
    assert conn.calls[1][0] == 'DROP DATABASE IF EXISTS "smartfinance_eval_ab_x" WITH (FORCE)'


def test_refuses_to_touch_databases_without_the_prefix(conn):
    """The real database can never be created or dropped."""
    with pytest.raises(ValueError, match="eval database"):
        evaldb._drop(ADMIN, "smart_finance")
    with pytest.raises(ValueError, match="eval database"):
        evaldb._create(ADMIN, "smart_finance")
    assert conn.calls == []


def test_drop_databases_for_one_run_only_matches_that_run(conn):
    """The LIKE pattern is escaped and scoped to the run key."""
    conn.answers["pg_database"] = [("smartfinance_eval_ab12cd34_christian",)]

    dropped = evaldb.drop_databases(ADMIN, "eval-1")

    assert dropped == ["smartfinance_eval_ab12cd34_christian"]
    assert conn.calls[0][1] == (f"smartfinance\\_eval\\_{evaldb.run_key('eval-1')}\\_%",)
    assert "DROP DATABASE" in conn.calls[1][0]


def test_drop_databases_without_a_run_sweeps_every_eval_database(conn):
    """No eval id means every eval database."""
    evaldb.drop_databases(ADMIN)

    assert conn.calls[0][1] == ("smartfinance\\_eval\\_%",)


def test_drop_databases_with_an_empty_run_id_is_still_run_scoped(conn):
    """An empty eval id is a run (key of ""), not "no run", so it never sweeps everything."""
    evaldb.drop_databases(ADMIN, "")

    assert conn.calls[0][1] == (f"smartfinance\\_eval\\_{evaldb.run_key('')}\\_%",)


def test_seed_runs_the_binary_with_only_the_chosen_persona(monkeypatch):
    """The temp fixtures file holds just the persona and the binary targets the eval DB."""
    calls, seen = [], {}

    def fake_run(cmd, **kwargs):
        calls.append((cmd, kwargs))
        if "--fixtures" in cmd:
            seen["json"] = Path(cmd[cmd.index("--fixtures") + 1]).read_text()
        return SimpleNamespace(returncode=0, stdout="", stderr="")

    monkeypatch.setattr(evaldb.subprocess, "run", fake_run)

    evaldb._seed("postgres://x/smartfinance_eval_ab_x", PERSONA)

    assert calls[0][0][:4] == ["cargo", "build", "--bin", "seed-sandbox"]
    assert evaldb.json.loads(seen["json"]) == [PERSONA]
    assert calls[1][1]["env"]["DATABASE_URL"] == "postgres://x/smartfinance_eval_ab_x"


def test_migrate_points_sqlx_at_the_eval_database(monkeypatch):
    """Sqlx gets the migrations folder and the eval DB url."""
    calls = []
    monkeypatch.setattr(
        evaldb.subprocess,
        "run",
        lambda cmd, **kw: calls.append(cmd) or SimpleNamespace(returncode=0, stdout="", stderr=""),
    )

    evaldb._migrate("postgres://x/smartfinance_eval_ab_x")

    assert calls[0][:3] == ["sqlx", "migrate", "run"]
    assert "postgres://x/smartfinance_eval_ab_x" in calls[0]


def test_run_failure_includes_the_command_output(monkeypatch):
    """A failing subprocess raises with the tail of its output."""
    monkeypatch.setattr(
        evaldb.subprocess,
        "run",
        lambda cmd, **kw: SimpleNamespace(returncode=1, stdout="", stderr="plaid said no"),
    )

    with pytest.raises(RuntimeError, match="plaid said no"):
        evaldb._run(["seed-sandbox", "--fixtures", "x"])


def test_user_id_is_none_when_the_database_is_missing(monkeypatch):
    """A connection error means the persona is not seeded yet."""

    def boom(*a, **k):
        raise psycopg.OperationalError("does not exist")

    monkeypatch.setattr(evaldb.psycopg, "connect", boom)

    assert evaldb._user_id(EVAL_URL, "a@test.com") is None


def test_ensure_persona_reuses_a_seeded_database(monkeypatch):
    """When the persona already has a linked user nothing is rebuilt."""
    monkeypatch.setattr(evaldb, "_user_id", lambda url, email: 7)
    monkeypatch.setattr(evaldb, "_drop", lambda *a: pytest.fail("must not drop"))

    persona_db = evaldb.ensure_persona(ADMIN, "eval-1", PERSONA)

    assert persona_db.user_id == 7
    assert persona_db.url.endswith("/" + persona_db.name)


def test_ensure_persona_builds_seeds_and_silences_notifications(monkeypatch, conn):
    """A missing persona is created, migrated, seeded, and its notifications turned off."""
    steps, ids = [], iter([None, 7])
    monkeypatch.setattr(evaldb, "_user_id", lambda url, email: next(ids))
    for step in ("_drop", "_create"):
        monkeypatch.setattr(evaldb, step, lambda admin, name, s=step: steps.append(s))
    monkeypatch.setattr(evaldb, "_migrate", lambda url: steps.append("_migrate"))
    monkeypatch.setattr(evaldb, "_seed", lambda url, persona: steps.append("_seed"))

    persona_db = evaldb.ensure_persona(ADMIN, "eval-1", PERSONA)

    assert steps == ["_drop", "_create", "_migrate", "_seed"]
    assert persona_db.user_id == 7
    assert "email_notifications = false" in conn.calls[0][0]
    assert conn.calls[0][1] == (7,)


def test_ensure_persona_errors_when_seeding_creates_no_user(monkeypatch):
    """A seed that produced no linked user is a hard error."""
    monkeypatch.setattr(evaldb, "_user_id", lambda url, email: None)
    for step in ("_drop", "_create"):
        monkeypatch.setattr(evaldb, step, lambda *a: None)
    monkeypatch.setattr(evaldb, "_migrate", lambda url: None)
    monkeypatch.setattr(evaldb, "_seed", lambda url, persona: None)

    with pytest.raises(RuntimeError, match="no linked user"):
        evaldb.ensure_persona(ADMIN, "eval-1", PERSONA)


def test_account_ids_maps_name_to_plaid_id(conn):
    """Account names resolve to Plaid ids."""
    conn.answers["FROM plaid_accounts"] = [("TD Bank Student Checking", "acc_1")]

    assert evaldb.account_ids(EVAL_URL, 7) == {"TD Bank Student Checking": "acc_1"}


def test_resolve_account_names_the_choices_on_error():
    """An unknown account name lists the persona's accounts."""
    assert evaldb.resolve_account(ACCOUNTS, "Discover it Student") == "acc_2"
    with pytest.raises(DefinitionError, match="Discover it Student"):
        evaldb.resolve_account(ACCOUNTS, "Nope")


def test_reset_clears_then_inserts_the_definitions(conn):
    """Old state is deleted, then budgets, rules (with accounts) and goals are inserted."""
    conn.answers["INSERT INTO rules"] = [(5,)]
    conn.answers["SELECT COALESCE"] = [(1000.0,)]

    evaldb.reset(EVAL_URL, 7, make_case(), ACCOUNTS)

    statements = [text.split(" (")[0].split(" WHERE")[0] for text, _ in conn.calls]
    assert statements[:4] == [
        "DELETE FROM budgets",
        "DELETE FROM rules",
        "DELETE FROM goals",
        "DELETE FROM transaction_analyses",
    ]
    by_text = {text.split(" VALUES")[0]: params for text, params in conn.calls}
    assert (7, "Eating out", "FOOD_DINING", 100.0) in by_text.values()
    rule_accounts = [p for t, p in conn.calls if "INSERT INTO rule_accounts" in t]
    assert rule_accounts == [(5, "acc_1")]
    goal = next(p for t, p in conn.calls if "INSERT INTO goals" in t)
    assert goal == (
        7,
        "acc_1",
        "Trip",
        "Save",
        2500.0,
        1000.0,
        date.today() + timedelta(days=120),
    )


def test_reset_rejects_an_unknown_account_before_committing(conn):
    """A goal on an unknown account raises DefinitionError."""
    conn.answers["INSERT INTO rules"] = [(5,)]
    case = make_case(goals=[{"name": "G", "account": "Nope", "target_amount": 5}])

    with pytest.raises(DefinitionError, match="unknown account"):
        evaldb.reset(EVAL_URL, 7, case, ACCOUNTS)


def test_functions_refuse_the_real_database_url_before_connecting(monkeypatch):
    """reset, account_ids, _migrate and _seed reject a non-eval URL without any I/O."""
    touched = []
    monkeypatch.setattr(evaldb.psycopg, "connect", lambda *a, **k: touched.append("connect"))
    monkeypatch.setattr(evaldb.subprocess, "run", lambda *a, **k: touched.append("run"))

    with pytest.raises(ValueError, match="eval database"):
        evaldb.reset(ADMIN, 7, make_case(), ACCOUNTS)
    with pytest.raises(ValueError, match="eval database"):
        evaldb.account_ids(ADMIN, 7)
    with pytest.raises(ValueError, match="eval database"):
        evaldb._migrate(ADMIN)
    with pytest.raises(ValueError, match="eval database"):
        evaldb._seed(ADMIN, PERSONA)
    assert touched == []


def test_check_url_follows_a_dbname_query_override():
    """The guard looks at the database libpq would use, not just the URL path."""
    evaldb._check_url(EVAL_URL)
    with pytest.raises(ValueError, match="eval database"):
        evaldb._check_url("postgres://x/smartfinance_eval_ab_x?dbname=smart_finance")


def test_ensure_persona_checks_the_url_before_updating_users(monkeypatch):
    """The users UPDATE step is guarded: a URL that resolves to the real database is refused."""
    monkeypatch.setattr(evaldb, "database_url", lambda admin, name: ADMIN)
    monkeypatch.setattr(evaldb, "_user_id", lambda url, email: pytest.fail("must not query"))

    with pytest.raises(ValueError, match="eval database"):
        evaldb.ensure_persona(ADMIN, "eval-1", PERSONA)


def test_reset_removes_old_seeded_transactions_after_the_four_standard_deletes(conn):
    """Seeded rows from a previous test are deleted; the first four deletes are unchanged."""
    evaldb.reset(EVAL_URL, 7, make_case(rules=[], goals=[]), ACCOUNTS)

    deletes = [(t, p) for t, p in conn.calls if t.startswith("DELETE FROM plaid_transactions")]
    assert deletes == [
        ("DELETE FROM plaid_transactions WHERE transaction_id LIKE %s", (evaldb.SEED_PREFIX + "%",))
    ]
    texts = [t for t, _ in conn.calls]
    assert texts.index(deletes[0][0]) == 4


def test_reset_inserts_seeded_transactions_with_dates_and_categories(conn):
    """Each seed becomes a plaid_transactions row dated `days_ago` days back."""
    case = make_case(
        rules=[],
        goals=[],
        seed_transactions=[
            {
                "name": "Chipotle",
                "amount": 14.5,
                "account": "Discover it Student",
                "category": "FOOD_DINING",
                "days_ago": 2,
            },
            {"name": "Payroll", "amount": -1500, "account": "TD Bank Student Checking"},
        ],
    )

    evaldb.reset(EVAL_URL, 7, case, ACCOUNTS)

    inserts = [p for t, p in conn.calls if t.startswith("INSERT INTO plaid_transactions")]
    assert inserts == [
        (
            f"{evaldb.SEED_PREFIX}0",
            "acc_2",
            "Chipotle",
            14.5,
            date.today() - timedelta(days=2),
            "FOOD_DINING",
        ),
        (f"{evaldb.SEED_PREFIX}1", "acc_1", "Payroll", -1500.0, date.today(), "OTHER"),
    ]


def test_reset_rejects_a_seed_on_an_unknown_account(conn):
    """A typo in the account name is a definition error, not a database error."""
    case = make_case(
        rules=[],
        goals=[],
        seed_transactions=[{"name": "x", "amount": 1, "account": "Nope"}],
    )

    with pytest.raises(DefinitionError, match="unknown account"):
        evaldb.reset(EVAL_URL, 7, case, ACCOUNTS)


def test_reset_always_restores_stashed_transactions_first(conn):
    """Rows set aside by an earlier seeded test come back before anything else is inserted."""
    evaldb.reset(EVAL_URL, 7, make_case(rules=[], goals=[]), ACCOUNTS)

    texts = [t for t, _ in conn.calls]
    restore = next(i for i, t in enumerate(texts) if "INSERT INTO plaid_transactions SELECT" in t)
    assert "eval_stashed_transactions" in texts[restore]
    assert restore > texts.index("DELETE FROM plaid_transactions WHERE transaction_id LIKE %s")
    assert not any("INSERT INTO eval_stashed_transactions" in t for t in texts)


def test_reset_stashes_the_persona_transactions_only_when_a_case_seeds_some(conn):
    """A seeded case sets aside the accounts' own rows so its totals are exact."""
    case = make_case(
        rules=[],
        goals=[],
        seed_transactions=[
            {"name": "Payroll", "amount": -1500, "account": "TD Bank Student Checking"}
        ],
    )

    evaldb.reset(EVAL_URL, 7, case, ACCOUNTS)

    stash = [(t, p) for t, p in conn.calls if "INSERT INTO eval_stashed_transactions" in t]
    assert len(stash) == 1
    assert stash[0][1] == (["acc_1", "acc_2"], evaldb.SEED_PREFIX + "%")
    texts = [t for t, _ in conn.calls]
    assert texts.index(stash[0][0]) < texts.index(
        next(t for t in texts if t.startswith("INSERT INTO plaid_transactions ("))
    )
