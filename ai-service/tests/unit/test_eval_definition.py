"""Tests for parsing and validating eval test definitions."""

import json

import pytest

from definition import DefinitionError, load_personas, parse

PERSONAS = {"christian@test.com": {"email": "christian@test.com", "first_name": "Christian"}}
CHECKING = "TD Bank Student Checking"


def analyze_vars(**overrides):
    """Return valid `vars` for an analyze test, with optional overrides."""
    base = {
        "persona": "christian@test.com",
        "agent": "analyze",
        "transaction": {"name": "Best Buy", "amount": 900, "account": CHECKING},
        "expected": "ALERT",
        "__evalId": "ignored-promptfoo-var",
    }
    base.update(overrides)
    return base


def test_parse_analyze_applies_defaults_and_ignores_promptfoo_vars():
    """An analyze test needs only persona, agent and a transaction; the rest has defaults."""
    case = parse(analyze_vars(), PERSONAS)

    assert case.agent == "analyze"
    assert case.persona == PERSONAS["christian@test.com"]
    assert case.transaction.category == "OTHER"
    assert case.transaction.days_ago == 0
    assert case.transaction.amount == 900.0
    assert case.goals == () and case.budgets == () and case.rules == ()
    assert case.message is None
    assert case.expected == "ALERT"


def test_parse_chat_case():
    """A chat test needs a message and no transaction."""
    case = parse({"persona": "christian@test.com", "agent": "chat", "message": "Hi?"}, PERSONAS)

    assert case.agent == "chat"
    assert case.message == "Hi?"
    assert case.transaction is None


def test_parse_goals_budgets_and_rules():
    """Goals, budgets and rules become typed definitions; a rule's account is optional."""
    case = parse(
        analyze_vars(
            goals=[
                {
                    "name": "Trip",
                    "description": "Save",
                    "account": CHECKING,
                    "target_amount": 2500,
                    "days_until_target": 120,
                }
            ],
            budgets=[{"name": "Eating out", "category": "FOOD_DINING", "monthly_limit": 100}],
            rules=[
                {"description": "No gambling."},
                {"description": "Flag over $200.", "account": CHECKING},
            ],
        ),
        PERSONAS,
    )

    assert case.goals[0].target_amount == 2500.0 and case.goals[0].days_until_target == 120
    assert case.budgets[0].category == "FOOD_DINING"
    assert [r.account for r in case.rules] == [None, CHECKING]


@pytest.mark.parametrize(
    ("overrides", "message"),
    [
        ({"persona": "nobody@test.com"}, "unknown persona"),
        ({"persona": None}, "`persona` is required"),
        ({"agent": "oracle"}, "agent must be one of"),
        ({"transaction": None}, "`transaction` is required"),
        ({"transaction": {"name": "X", "account": CHECKING}}, "`amount` is required"),
        ({"transaction": {"name": "X", "amount": "lots", "account": CHECKING}}, "must be a number"),
        (
            {"transaction": {"name": "X", "amount": 1, "account": CHECKING, "category": "FUN"}},
            "category must be one of",
        ),
        (
            {"budgets": [{"name": "B", "category": "FUN", "monthly_limit": 5}]},
            "category must be one of",
        ),
        (
            {"budgets": [{"name": "B", "category": "OTHER", "monthly_limit": 0}]},
            "must be greater than 0",
        ),
        (
            {
                "budgets": [
                    {"name": "A", "category": "OTHER", "monthly_limit": 5},
                    {"name": "B", "category": "OTHER", "monthly_limit": 6},
                ]
            },
            "one budget per category",
        ),
        (
            {"goals": [{"name": "G", "account": CHECKING, "target_amount": -1}]},
            "must be greater than 0",
        ),
        ({"rules": [{"account": CHECKING}]}, "`description` is required"),
    ],
)
def test_parse_rejects_invalid_definitions(overrides, message):
    """Every invalid definition fails with a message that names the problem."""
    with pytest.raises(DefinitionError, match=message):
        parse(analyze_vars(**overrides), PERSONAS)


def test_parse_chat_requires_a_message():
    """A chat test without a message is invalid."""
    with pytest.raises(DefinitionError, match="`message` is required"):
        parse({"persona": "christian@test.com", "agent": "chat"}, PERSONAS)


def test_load_personas_reads_a_fixtures_file_keyed_by_email(tmp_path):
    """Personas are keyed by email."""
    path = tmp_path / "users.json"
    path.write_text(json.dumps([{"email": "a@test.com"}, {"email": "b@test.com"}]))

    assert set(load_personas(path)) == {"a@test.com", "b@test.com"}


def test_load_personas_default_is_the_sandbox_fixture():
    """With no argument the repo's sandbox users are loaded."""
    assert "christian@test.com" in load_personas()
