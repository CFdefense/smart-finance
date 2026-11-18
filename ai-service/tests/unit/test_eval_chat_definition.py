"""Tests for the chat-only eval vars: follow-ups, backend mode, seeds, stubs and assertion inputs."""

import pytest

import definition
from agent.pending import PendingActionStore
from agent.tools import ANALYTICS_TOOLS, READ_TOOLS, make_write_tools, web_search
from definition import DefinitionError, parse
from rig import RecordingBackendApi

PERSONAS = {"c@test.com": {"email": "c@test.com"}}
BASE = {"persona": "c@test.com", "agent": "chat", "message": "hi"}


def case(**extra):
    """Parse a chat case with `extra` vars."""
    return parse({**BASE, **extra}, PERSONAS)


def test_chat_defaults_are_single_turn_ok_backend_real_web():
    """Existing evals keep working: nothing extra is required."""
    c = case()
    assert c.followups == () and c.backend == "ok"
    assert c.seed_transactions == () and c.search_results is None


def test_followups_are_kept_in_order():
    """Follow-up messages are sent after the first message."""
    assert case(followups=["yes", "thanks"]).followups == ("yes", "thanks")


@pytest.mark.parametrize("bad", ["", 3, [""], [1]])
def test_followups_must_be_non_empty_strings(bad):
    """A malformed follow-up list is rejected."""
    with pytest.raises(DefinitionError, match="followups"):
        case(followups=bad)


@pytest.mark.parametrize("mode", ["ok", "conflict", "down"])
def test_backend_modes(mode):
    """The three fake backend behaviours are accepted."""
    assert case(backend=mode).backend == mode


def test_unknown_backend_mode_is_rejected():
    """Typos in the mode fail loudly."""
    with pytest.raises(DefinitionError, match="backend must be one of"):
        case(backend="flaky")


def test_seed_transactions_are_parsed_like_the_analyzed_transaction():
    """Same fields and defaults as `transaction`: category OTHER, days_ago 0."""
    c = case(
        seed_transactions=[
            {"name": "Chipotle", "amount": 14.5, "account": "Checking", "category": "FOOD_DINING"},
            {"name": "Rent", "amount": 900, "account": "Checking", "days_ago": 3},
        ]
    )
    first, second = c.seed_transactions
    assert (first.name, first.amount, first.category, first.days_ago) == (
        "Chipotle",
        14.5,
        "FOOD_DINING",
        0,
    )
    assert (second.category, second.days_ago) == ("OTHER", 3)


def test_seed_transaction_errors_name_the_index():
    """The error says which seed row is wrong."""
    with pytest.raises(DefinitionError, match=r"seed_transactions\[1\]"):
        case(
            seed_transactions=[
                {"name": "a", "amount": 1, "account": "x"},
                {"name": "b", "amount": 1, "account": "x", "category": "BOATS"},
            ]
        )


def test_search_results_stub_and_the_empty_stub_differ_from_the_real_web():
    """None means real search; an empty list means a stub that finds nothing."""
    stub = case(search_results=[{"title": "T", "url": "https://x.test", "content": "C"}])
    assert [r.title for r in stub.search_results] == ["T"]
    assert case(search_results=[]).search_results == ()


def test_search_result_needs_content():
    """A stub result without content is meaningless."""
    with pytest.raises(DefinitionError, match=r"search_results\[0\]"):
        case(search_results=[{"title": "T", "url": "u"}])


@pytest.mark.parametrize("key", ["expect_tools", "forbid_tools"])
def test_unknown_tool_names_are_rejected(key):
    """A misspelled tool name would make an assertion vacuous."""
    with pytest.raises(DefinitionError, match="unknown tool"):
        case(**{key: ["propose_budjet"]})


def test_known_tool_names_are_accepted():
    """Real tool names pass."""
    case(expect_tools=["list_budgets", "propose_budget"], forbid_tools=["confirm_action"])


def test_expect_backend_accepts_exact_path_and_prefix_forms():
    """Creates use an exact path; updates and deletes use a prefix because ids vary."""
    case(
        expect_backend=[
            {"method": "POST", "path": "/api/budgets", "body": {"category": "GROCERIES"}},
            {"method": "DELETE", "path_prefix": "/api/rules/", "turn": 1},
        ]
    )


@pytest.mark.parametrize(
    "item",
    [
        {"method": "GET", "path": "/api/budgets"},
        {"method": "POST"},
        {"method": "POST", "path": "/a", "path_prefix": "/a"},
        {"method": "POST", "path": "/a", "body": "x"},
        {"method": "POST", "path": "/a", "turn": "one"},
        "POST /api/budgets",
    ],
)
def test_expect_backend_rejects_malformed_items(item):
    """Each malformed expectation is a definition error."""
    with pytest.raises(DefinitionError, match=r"expect_backend\[0\]"):
        case(expect_backend=[item])


@pytest.mark.parametrize("key", ["must_contain", "must_not_contain", "private_terms"])
def test_string_list_assertion_inputs_are_validated(key):
    """These must be lists of non-empty strings."""
    case(**{key: ["a", "b"]})
    with pytest.raises(DefinitionError, match=key):
        case(**{key: "a"})


@pytest.mark.parametrize(
    "key,value",
    [
        ("followups", ["x"]),
        ("backend", "down"),
        ("seed_transactions", [{"name": "a", "amount": 1, "account": "x"}]),
        ("search_results", []),
        ("expect_tools", ["list_budgets"]),
        ("forbid_tools", ["confirm_action"]),
        ("expect_backend", [{"method": "POST", "path": "/a"}]),
        ("private_terms", ["x"]),
    ],
)
def test_chat_only_vars_are_rejected_for_the_analyzer(key, value):
    """The analyzer takes none of the chat-only vars."""
    analyze = {
        "persona": "c@test.com",
        "agent": "analyze",
        "transaction": {"name": "a", "amount": 1, "account": "x"},
    }
    with pytest.raises(DefinitionError, match=key):
        parse({**analyze, key: value}, PERSONAS)


def test_known_tools_are_exactly_the_chat_agents_tools():
    """KNOWN_TOOLS cannot drift from the tools ChatAgent registers."""
    tools = [
        web_search,
        *READ_TOOLS,
        *ANALYTICS_TOOLS,
        *make_write_tools(PendingActionStore(), RecordingBackendApi()),
    ]
    assert set(definition.KNOWN_TOOLS) == {t.name for t in tools}
    assert len(definition.KNOWN_TOOLS) == len(tools) == 19
