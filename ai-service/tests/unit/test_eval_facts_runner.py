"""Tests for the facts snapshot and the in-process agent runner."""

import json
import os
from datetime import date, timedelta
from types import SimpleNamespace

import pytest

import facts
import runner
from agent import AnalysisResult, ChatResponse
from db import Account, Budget, Goal, Rule, Transaction
from definition import parse

PERSONAS = {"c@test.com": {"email": "c@test.com"}}
ACCOUNTS = {"Checking": "acc_1"}


def make_case(**test_vars):
    """Build an EvalCase for persona c@test.com."""
    return parse({"persona": "c@test.com", **test_vars}, PERSONAS)


def test_snapshot_collects_everything_a_grader_needs(monkeypatch):
    """Rules are merged across accounts and show account names, not ids."""
    today = date(2026, 11, 10)
    monkeypatch.setattr(
        facts,
        "load_accounts",
        lambda uid: [
            Account(account_id="acc_1", name="Checking", account_type="depository"),
            Account(account_id="acc_2", name="Card", account_type="credit"),
        ],
    )
    rules = {
        "acc_1": [
            Rule(id=1, description="No gambling.", account_ids=[]),
            Rule(id=2, description="Over $200.", account_ids=["acc_1"]),
        ],
        "acc_2": [Rule(id=1, description="No gambling.", account_ids=[])],
    }
    monkeypatch.setattr(facts, "load_rules", lambda uid, account_id: rules[account_id])
    seen = {}

    def fake_transactions(uid, since=None, limit=None):
        seen.update(since=since, limit=limit)
        return [Transaction(transaction_id="t1", name="Chipotle", amount=14.5, date=today)]

    monkeypatch.setattr(facts, "load_transactions", fake_transactions)
    monkeypatch.setattr(
        facts,
        "load_budgets",
        lambda uid, d: [
            Budget(
                name="Eating out", category="FOOD_DINING", monthly_limit=100, spent_this_month=82
            )
        ],
    )
    monkeypatch.setattr(
        facts,
        "load_goals",
        lambda uid: [
            Goal(name="Trip", account_name="Checking", target_amount=2500, baseline_balance=100)
        ],
    )

    snap = facts.snapshot(7, today)

    json.dumps(snap)  # must be JSON-serializable for promptfoo metadata
    assert snap["today"] == "2026-11-10"
    assert [a["name"] for a in snap["accounts"]] == ["Checking", "Card"]
    assert snap["rules"] == [
        {"description": "No gambling.", "applies_to": "all accounts"},
        {"description": "Over $200.", "applies_to": ["Checking"]},
    ]
    assert snap["budgets"][0]["spent_this_month"] == 82
    assert snap["goals"][0]["name"] == "Trip"
    assert snap["transactions"][0]["name"] == "Chipotle"
    assert seen["since"] == today - timedelta(days=facts.RECENT_DAYS)
    assert seen["limit"] == facts.MAX_TRANSACTIONS


class FakeAnalyzer:
    """Captures the request and returns a canned result."""

    def __init__(self):
        self.request = None

    def analyze(self, request):
        """Record the request and return an ANALYZED result."""
        self.request = request
        return AnalysisResult(transaction_id=request.transaction.transaction_id, status="ANALYZED")


class FakeChat:
    """Captures the request and returns a canned reply."""

    def __init__(self):
        self.request = None

    def generate_answer(self, request):
        """Record the request and answer."""
        self.request = request
        return ChatResponse(response="Yes, you can.")


def test_run_analyze_builds_a_request_and_returns_the_result_json(monkeypatch):
    """The transaction is dated days_ago, uses the resolved account id and a fresh id."""
    agent = FakeAnalyzer()
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)
    case = make_case(
        agent="analyze",
        transaction={
            "name": "Best Buy",
            "amount": 900,
            "account": "Checking",
            "category": "SHOPPING",
            "days_ago": 2,
        },
    )

    output = runner.run(case, "postgres://x/smartfinance_eval_ab_c", 7, ACCOUNTS)

    request = agent.request
    assert request.user_id == 7
    assert request.transaction.account_id == "acc_1"
    assert request.transaction.date == date.today() - timedelta(days=2)
    assert request.transaction.transaction_id.startswith("eval-")
    assert request.transaction.pending is False
    assert json.loads(output)["status"] == "ANALYZED"


def test_run_chat_returns_the_reply_text(monkeypatch):
    """Chat sends the message for the user and returns just the text."""
    agent = FakeChat()
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)

    output = runner.run(make_case(agent="chat", message="Can I afford a TV?"), "u", 7, ACCOUNTS)

    assert output == "Yes, you can."
    assert (agent.request.user_id, agent.request.message) == (7, "Can I afford a TV?")


def test_run_points_the_agents_at_the_eval_database_then_restores(monkeypatch):
    """DATABASE_URL is the eval database during the run and the original afterwards."""
    monkeypatch.setenv("DATABASE_URL", "postgres://real/smart_finance")
    seen = {}

    class Spy(FakeChat):
        def generate_answer(self, request):
            """Capture the database url the agent would connect to."""
            seen["url"] = os.environ["DATABASE_URL"]
            return super().generate_answer(request)

    monkeypatch.setattr(runner, "_agent", lambda kind: Spy())

    runner.run(make_case(agent="chat", message="hi"), "postgres://eval/db", 7, ACCOUNTS)

    assert seen["url"] == "postgres://eval/db"
    assert os.environ["DATABASE_URL"] == "postgres://real/smart_finance"


def test_run_restores_the_database_url_when_the_agent_raises(monkeypatch):
    """The environment is restored even if the run blows up."""
    monkeypatch.setenv("DATABASE_URL", "postgres://real/smart_finance")

    class Boom:
        def generate_answer(self, request):
            """Fail."""
            raise RuntimeError("boom")

    monkeypatch.setattr(runner, "_agent", lambda kind: Boom())

    with pytest.raises(RuntimeError):
        runner.run(make_case(agent="chat", message="hi"), "postgres://eval/db", 7, ACCOUNTS)
    assert os.environ["DATABASE_URL"] == "postgres://real/smart_finance"


def test_agent_requires_an_openai_key(monkeypatch):
    """Without a key the runner fails clearly instead of building a broken agent."""
    runner._agents.clear()
    monkeypatch.setattr(runner, "load_dotenv", lambda *a, **k: None)
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)

    with pytest.raises(RuntimeError, match="OPENAI_API_KEY"):
        runner._agent("chat")


def test_agent_is_built_once_per_kind(monkeypatch, tmp_path):
    """Agents are expensive to build, so they are cached for the process."""
    runner._agents.clear()
    monkeypatch.setattr(runner, "load_dotenv", lambda *a, **k: None)
    monkeypatch.setenv("OPENAI_API_KEY", "sk-test")
    monkeypatch.setenv("AGENT_MODEL", "m")
    # Pre-set so the runner does not create a temp dir or leave the variable set.
    monkeypatch.setenv("AGENT_LOG_DIR", str(tmp_path))
    built = []
    monkeypatch.setattr(
        runner, "ChatAgent", lambda key, model: built.append((key, model)) or SimpleNamespace()
    )

    first = runner._agent("chat")

    assert runner._agent("chat") is first
    assert built == [("sk-test", "m")]
    runner._agents.clear()
