"""Tests for the promptfoo provider and hooks, with the database and agents faked."""

import pytest

import hooks
import provider
from definition import DefinitionError
from evaldb import PersonaDb

PERSONA_DB = PersonaDb(name="smartfinance_eval_ab_christian", url="postgres://x/y", user_id=7)
CONTEXT = {
    "vars": {
        "persona": "christian@test.com",
        "agent": "chat",
        "message": "hi",
        "__evalId": "eval-1",
    }
}


@pytest.fixture
def calls(monkeypatch):
    """Fake every collaborator and record the order they are called in."""
    log = []
    monkeypatch.setattr(provider.evaldb, "admin_url", lambda: "postgres://admin/real")

    def ensure(admin, eval_id, persona):
        log.append(("ensure", admin, eval_id, persona["email"]))
        return PERSONA_DB

    monkeypatch.setattr(provider.evaldb, "ensure_persona", ensure)
    monkeypatch.setattr(provider.evaldb, "account_ids", lambda url, uid: {"Checking": "acc_1"})
    monkeypatch.setattr(
        provider.evaldb, "reset", lambda url, uid, case, accounts: log.append(("reset", uid))
    )
    monkeypatch.setattr(
        provider.runner,
        "run",
        lambda case, url, uid, accounts: log.append(("run", url, uid)) or "the answer",
    )

    class FakeDatabase:
        def __init__(self, url):
            log.append(("database", url))

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

    monkeypatch.setattr(provider.runner, "database", FakeDatabase)
    monkeypatch.setattr(
        provider.facts, "snapshot", lambda uid: log.append(("facts", uid)) or {"rules": []}
    )
    return log


def test_call_api_prepares_the_database_runs_the_agent_and_returns_facts(calls):
    """Order: seed persona, reset definitions, run agent, snapshot facts from the eval DB."""
    response = provider.call_api("chat: hi", {}, CONTEXT)

    assert [c[0] for c in calls] == ["ensure", "reset", "run", "database", "facts"]
    assert calls[0] == ("ensure", "postgres://admin/real", "eval-1", "christian@test.com")
    assert response == {
        "output": "the answer",
        "metadata": {
            "facts": {"rules": []},
            "persona": "christian@test.com",
            "agent": "chat",
        },
    }


def test_call_api_reports_a_bad_definition_as_an_error_without_touching_the_database(calls):
    """A DefinitionError becomes an error response before any database work."""
    bad = {"vars": {**CONTEXT["vars"], "agent": "oracle"}}

    response = provider.call_api("x", {}, bad)

    assert "agent must be one of" in response["error"]
    assert calls == []


def test_call_api_reports_an_unknown_account_as_an_error(calls, monkeypatch):
    """A definition error raised during reset (unknown account name) is also an error response."""

    def reset(url, uid, case, accounts):
        raise DefinitionError("unknown account 'Nope'")

    monkeypatch.setattr(provider.evaldb, "reset", reset)

    assert "unknown account" in provider.call_api("x", {}, CONTEXT)["error"]


def test_call_api_lets_infrastructure_failures_propagate(calls, monkeypatch):
    """A seed or Plaid failure is an exception, not a failed grade."""

    def ensure(admin, eval_id, persona):
        raise RuntimeError("plaid said no")

    monkeypatch.setattr(provider.evaldb, "ensure_persona", ensure)

    with pytest.raises(RuntimeError, match="plaid said no"):
        provider.call_api("x", {}, CONTEXT)


def test_hooks_sweep_everything_before_all_and_drop_the_run_after_all(monkeypatch):
    """BeforeAll removes leftovers from interrupted runs; afterAll removes this run's databases."""
    drops = []
    monkeypatch.setattr(hooks.evaldb, "admin_url", lambda: "postgres://admin/real")
    monkeypatch.setattr(
        hooks.evaldb, "drop_databases", lambda admin, eval_id=None: drops.append(eval_id) or []
    )

    hooks.on_event("beforeAll", {"suite": {}})
    hooks.on_event("beforeEach", {"test": {}})
    hooks.on_event("afterAll", {"evalId": "eval-1"})

    assert drops == [None, "eval-1"]
