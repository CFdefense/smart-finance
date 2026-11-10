"""Create, seed, reset and drop the per-persona databases that evals run against."""

import hashlib
import json
import os
import re
import subprocess
import tempfile
from dataclasses import dataclass
from datetime import date, timedelta
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

import psycopg
from dotenv import find_dotenv, load_dotenv
from psycopg import sql
from psycopg.conninfo import conninfo_to_dict

from definition import DefinitionError, EvalCase

PREFIX = "smartfinance_eval_"
REPO = Path(__file__).resolve().parents[2]
MIGRATIONS = REPO / "migrations"
SEED_BIN = REPO / "target" / "debug" / "seed-sandbox"


@dataclass(frozen=True)
class PersonaDb:
    """One seeded eval database and the persona's user id inside it."""

    name: str
    url: str
    user_id: int


def admin_url() -> str:
    """Return the real database URL. It is only used to create and drop eval databases."""
    load_dotenv(find_dotenv(usecwd=True))
    return os.environ["DATABASE_URL"]


def run_key(eval_id: str) -> str:
    """Return a short stable key for one promptfoo run."""
    return hashlib.sha256(eval_id.encode()).hexdigest()[:8]


def db_name(eval_id: str, email: str) -> str:
    """Return the eval database name for a persona in a run."""
    local = re.sub(r"[^a-z0-9]+", "_", email.split("@")[0].lower())[:30]
    return f"{PREFIX}{run_key(eval_id)}_{local}"


def database_url(url: str, name: str) -> str:
    """Return `url` pointed at database `name`, or raise if libpq would resolve elsewhere."""
    out = urlunsplit(urlsplit(url)._replace(path=f"/{name}"))
    if conninfo_to_dict(out).get("dbname") != name:
        raise ValueError(
            "DATABASE_URL must be a plain postgres://user:pw@host:port/db URL "
            "without a `dbname` override in its query string"
        )
    return out


def _check(name: str) -> None:
    """Refuse any name that is not an eval database, so the real one is never touched."""
    if not name.startswith(PREFIX):
        raise ValueError(f"{name!r} is not an eval database (must start with {PREFIX})")


def _check_url(url: str) -> None:
    """Refuse any URL whose resolved database is not an eval database."""
    if not (conninfo_to_dict(url).get("dbname") or "").startswith(PREFIX):
        raise ValueError(f"URL does not resolve to an eval database (must start with {PREFIX})")


def _create(admin: str, name: str) -> None:
    _check(name)
    with psycopg.connect(admin, autocommit=True) as conn:
        conn.execute(sql.SQL("CREATE DATABASE {}").format(sql.Identifier(name)))


def _drop(admin: str, name: str) -> None:
    _check(name)
    with psycopg.connect(admin, autocommit=True) as conn:
        conn.execute(
            sql.SQL("DROP DATABASE IF EXISTS {} WITH (FORCE)").format(sql.Identifier(name))
        )


def drop_databases(admin: str, eval_id: str | None = None) -> list[str]:
    """Drop the eval databases of one run (or every eval database) and return their names."""
    prefix = PREFIX + (run_key(eval_id) + "_" if eval_id is not None else "")
    pattern = prefix.replace("_", r"\_") + "%"
    with psycopg.connect(admin, autocommit=True) as conn:
        rows = conn.execute(
            "SELECT datname FROM pg_database WHERE datname LIKE %s", (pattern,)
        ).fetchall()
    names = [row[0] for row in rows]
    for name in names:
        _drop(admin, name)
    return names


def _run(cmd: list[str], **kwargs) -> None:
    """Run a command and raise with the tail of its output if it fails."""
    done = subprocess.run(cmd, capture_output=True, text=True, check=False, **kwargs)
    if done.returncode != 0:
        tail = (done.stderr or done.stdout).strip()[-500:]
        raise RuntimeError(f"`{' '.join(str(c) for c in cmd[:3])}` failed: {tail}")


def _migrate(url: str) -> None:
    _check_url(url)
    _run(["sqlx", "migrate", "run", "--source", str(MIGRATIONS), "--database-url", url])


def _seed(url: str, persona: dict) -> None:
    """Seed only `persona` from Plaid into the eval database using the existing binary."""
    _check_url(url)
    _run(["cargo", "build", "--bin", "seed-sandbox"], cwd=REPO)
    with tempfile.TemporaryDirectory() as tmp:
        fixtures = Path(tmp) / "persona.json"
        fixtures.write_text(json.dumps([persona]))
        _run(
            [str(SEED_BIN), "--fixtures", str(fixtures)],
            env={**os.environ, "DATABASE_URL": url},
            cwd=REPO,
        )


def _user_id(url: str, email: str) -> int | None:
    """Return the persona's user id if it has a linked bank connection in `url`, else None."""
    try:
        with psycopg.connect(url) as conn:
            row = conn.execute(
                "SELECT u.id FROM users u JOIN plaid_links l ON l.user_id = u.id "
                "WHERE u.email = %s LIMIT 1",
                (email,),
            ).fetchone()
    except psycopg.Error:
        return None
    return row[0] if row else None


def ensure_persona(admin: str, eval_id: str, persona: dict) -> PersonaDb:
    """Return the run's seeded database for `persona`, building it on first use."""
    name = db_name(eval_id, persona["email"])
    url = database_url(admin, name)
    _check_url(url)
    user_id = _user_id(url, persona["email"])
    if user_id is None:
        _drop(admin, name)
        _create(admin, name)
        _migrate(url)
        _seed(url, persona)
        user_id = _user_id(url, persona["email"])
        if user_id is None:
            raise RuntimeError(f"seeding {persona['email']} produced no linked user")
        with psycopg.connect(url) as conn:
            conn.execute(
                "UPDATE users SET email_notifications = false, sms_notifications = false "
                "WHERE id = %s",
                (user_id,),
            )
    return PersonaDb(name=name, url=url, user_id=user_id)


def account_ids(url: str, user_id: int) -> dict[str, str]:
    """Return the user's accounts as {account name: Plaid account id}."""
    _check_url(url)
    with psycopg.connect(url) as conn:
        rows = conn.execute(
            "SELECT a.name, a.account_id FROM plaid_accounts a "
            "JOIN plaid_links l ON l.item_id = a.item_id WHERE l.user_id = %s",
            (user_id,),
        ).fetchall()
    return dict(rows)


def resolve_account(accounts: dict[str, str], name: str) -> str:
    """Return the Plaid account id for an account name, or raise `DefinitionError`."""
    try:
        return accounts[name]
    except KeyError:
        raise DefinitionError(
            f"unknown account {name!r}; the persona has {sorted(accounts)}"
        ) from None


def reset(url: str, user_id: int, case: EvalCase, accounts: dict[str, str]) -> None:
    """Replace the user's goals, budgets, rules and analyses with the test's definitions."""
    _check_url(url)
    today = date.today()
    with psycopg.connect(url) as conn:
        for table in ("budgets", "rules", "goals", "transaction_analyses"):
            conn.execute(f"DELETE FROM {table} WHERE user_id = %s", (user_id,))

        for budget in case.budgets:
            conn.execute(
                "INSERT INTO budgets (user_id, name, category, monthly_limit) "
                "VALUES (%s, %s, %s, %s)",
                (user_id, budget.name, budget.category, budget.monthly_limit),
            )

        for rule in case.rules:
            account_id = resolve_account(accounts, rule.account) if rule.account else None
            rule_id = conn.execute(
                "INSERT INTO rules (user_id, description) VALUES (%s, %s) RETURNING id",
                (user_id, rule.description),
            ).fetchone()[0]
            if account_id:
                conn.execute(
                    "INSERT INTO rule_accounts (rule_id, account_id) VALUES (%s, %s)",
                    (rule_id, account_id),
                )

        for goal in case.goals:
            account_id = resolve_account(accounts, goal.account)
            baseline = conn.execute(
                "SELECT COALESCE(current_balance, 0) FROM plaid_accounts WHERE account_id = %s",
                (account_id,),
            ).fetchone()[0]
            target_date = (
                None
                if goal.days_until_target is None
                else today + timedelta(days=goal.days_until_target)
            )
            conn.execute(
                "INSERT INTO goals (user_id, account_id, name, description, target_amount, "
                "baseline_balance, target_date) VALUES (%s, %s, %s, %s, %s, %s, %s)",
                (
                    user_id,
                    account_id,
                    goal.name,
                    goal.description,
                    goal.target_amount,
                    baseline,
                    target_date,
                ),
            )
