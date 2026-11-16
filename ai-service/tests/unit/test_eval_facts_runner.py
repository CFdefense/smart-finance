"""Tests for the facts snapshot and the in-process agent runner."""

import json
import os
from datetime import date, timedelta
from types import SimpleNamespace

import httpx
import pytest

import facts
import runner
from agent import AnalysisResult, ChatResponse
from agent.tools import web_search
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

    result = runner.run(case, "postgres://x/smartfinance_eval_ab_c", 7, ACCOUNTS)

    request = agent.request
    assert request.user_id == 7
    assert request.transaction.account_id == "acc_1"
    assert request.transaction.date == date.today() - timedelta(days=2)
    assert request.transaction.transaction_id.startswith("eval-")
    assert request.transaction.pending is False
    assert json.loads(result.output)["status"] == "ANALYZED"
    assert result.tool_calls == [] and result.backend_calls == []


def test_run_chat_returns_the_reply_text(monkeypatch):
    """Chat sends the message for the user and returns just the text."""
    agent = FakeChat()
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)

    result = runner.run(make_case(agent="chat", message="Can I afford a TV?"), "u", 7, ACCOUNTS)

    assert result.output == "Yes, you can."
    assert (agent.request.user_id, agent.request.message) == (7, "Can I afford a TV?")
    assert agent.request.history == []


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
        runner,
        "ChatAgent",
        lambda key, model, **kw: built.append((key, model, kw)) or SimpleNamespace(),
    )

    first = runner._agent("chat")

    assert runner._agent("chat") is first
    [(key, model, kw)] = built
    assert (key, model) == ("sk-test", "m")
    assert kw["backend_api"] is runner.RIG.backend
    assert kw["pending"] is runner.RIG.pending
    assert kw["extra_callbacks"] == [runner.RIG.trace]
    runner._agents.clear()


class ScriptedChat:
    """Answers each message with the next canned reply and records every request."""

    def __init__(self, replies, on_message=None):
        self.requests, self._replies, self._on_message = [], iter(replies), on_message

    def generate_answer(self, request):
        """Record the request, run the side effect, and answer."""
        self.requests.append(request)
        if self._on_message:
            self._on_message(request)
        return ChatResponse(response=next(self._replies))


def test_follow_ups_are_sent_in_order_with_the_real_replies_as_history(monkeypatch):
    """Turn N sees the user's and the agent's earlier messages, exactly as the app would send."""
    agent = ScriptedChat(["Shall I add it?", "Done."])
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)
    case = make_case(agent="chat", message="Add a budget.", followups=["Yes."])

    result = runner.run(case, "u", 7, ACCOUNTS)

    first, second = agent.requests
    assert first.history == []
    assert [(m.role, m.content) for m in second.history] == [
        ("user", "Add a budget."),
        ("assistant", "Shall I add it?"),
    ]
    assert second.message == "Yes."
    assert result.output == "[Turn 1] Shall I add it?\n\n[Turn 2] Done."


def test_each_message_runs_under_its_own_turn_number(monkeypatch):
    """The rig is told the turn before every message, so calls are attributed correctly."""
    seen = []
    agent = ScriptedChat(["a", "b", "c"], on_message=lambda r: seen.append(runner.RIG.trace.turn))
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)

    runner.run(make_case(agent="chat", message="1", followups=["2", "3"]), "u", 7, ACCOUNTS)

    assert seen == [0, 1, 2]


def test_a_test_starts_with_a_clean_rig_in_its_backend_mode(monkeypatch):
    """Calls, pending changes and the mode from an earlier test never leak into the next."""
    runner.RIG.backend.send(7, "POST", "/api/budgets", {})
    runner.RIG.pending.add(7, "budget", {}, "old", "r0")
    modes = []
    agent = ScriptedChat(["ok"], on_message=lambda r: modes.append(runner.RIG.backend.mode))
    monkeypatch.setattr(runner, "_agent", lambda kind: agent)

    result = runner.run(make_case(agent="chat", message="hi", backend="down"), "u", 7, ACCOUNTS)

    assert modes == ["down"] and result.backend_calls == []
    assert len(runner.RIG.pending) == 0


def test_the_result_carries_copies_of_the_tool_and_backend_records(monkeypatch):
    """What the graders see is what happened during the run, and later runs cannot change it."""

    def act(request):
        runner.RIG.trace.on_tool_start({"name": "propose_budget"}, "", inputs={"category": "X"})
        runner.RIG.backend.send(7, "POST", "/api/budgets", {"name": "n"})

    monkeypatch.setattr(runner, "_agent", lambda kind: ScriptedChat(["ok"], on_message=act))

    result = runner.run(make_case(agent="chat", message="hi"), "u", 7, ACCOUNTS)
    runner.RIG.reset()

    assert [c["name"] for c in result.tool_calls] == ["propose_budget"]
    assert result.backend_calls[0]["path"] == "/api/budgets"


def test_stubbed_search_returns_only_the_given_results_and_restores_the_environment(monkeypatch):
    """The stub replaces Tavily, sets a dummy key for its duration, and cleans up."""
    monkeypatch.delenv("TAVILY_API_KEY", raising=False)
    real_post = httpx.post
    case = make_case(
        agent="chat",
        message="hi",
        search_results=[{"title": "IRS", "url": "https://irs.test", "content": "Limit is $7,000."}],
    )

    with runner.stubbed_search(case.search_results):
        text = web_search.invoke({"query": "roth ira limit"})
        assert "Limit is $7,000." in text and "https://irs.test" in text

    assert httpx.post is real_post
    assert "TAVILY_API_KEY" not in runner.os.environ


def test_stubbed_search_with_no_results_says_so(monkeypatch):
    """An empty stub is a search that finds nothing."""
    with runner.stubbed_search(()):
        assert web_search.invoke({"query": "anything"}) == "No results."


def test_no_stub_leaves_the_real_web_alone(monkeypatch):
    """`None` means the test wants the real search."""
    real_post = httpx.post
    monkeypatch.delenv("TAVILY_API_KEY", raising=False)

    with runner.stubbed_search(None):
        assert httpx.post is real_post
        assert "TAVILY_API_KEY" not in runner.os.environ


def test_stubbed_search_restores_an_existing_key_even_if_the_run_fails(monkeypatch):
    """A real key set by the developer survives a failing test."""
    monkeypatch.setenv("TAVILY_API_KEY", "real-key")

    try:
        with runner.stubbed_search(()):
            raise RuntimeError("boom")
    except RuntimeError:
        pass

    assert runner.os.environ["TAVILY_API_KEY"] == "real-key"


def test_search_stub_is_applied_around_every_turn(monkeypatch):
    """The stub is active while the agent answers, including follow-ups."""
    seen = []

    def check(request):
        seen.append(runner.os.environ.get("TAVILY_API_KEY"))

    monkeypatch.delenv("TAVILY_API_KEY", raising=False)
    monkeypatch.setattr(runner, "_agent", lambda kind: ScriptedChat(["a", "b"], on_message=check))
    case = make_case(agent="chat", message="1", followups=["2"], search_results=[])

    runner.run(case, "u", 7, ACCOUNTS)

    assert seen == ["eval-stub", "eval-stub"]
