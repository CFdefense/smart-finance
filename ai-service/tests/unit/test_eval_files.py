"""Guards for the eval files: every YAML parses, its assertions have inputs, nothing is dead config."""

import json
import re
from pathlib import Path

import pytest
import yaml

import graders
from definition import CATEGORIES, parse

TESTS = Path(__file__).resolve().parents[1]
EVALS = TESTS / "evals"
ARTHUR = {
    a["meta"]["name"]
    for a in json.loads((TESTS.parents[1] / "fixtures" / "arthur-accounts.json").read_text())[
        "override_accounts"
    ]
}
# The accounts the Plaid sandbox gives christian@test.com.
CHRISTIAN = {
    "Capital One Quicksilver",
    "Discover it Student",
    "Robinhood Investing",
    "Robinhood Roth IRA",
    "TD Bank Student Checking",
}
ACCOUNTS = {"christian@test.com": CHRISTIAN, "arthur@test.com": ARTHUR}
NAME = re.compile(r"^(\d{2})_(analyze|chat)_[a-z0-9_]+\.yaml$")
# Each deterministic grader and the test var it reads.
NEEDS = {
    "tools_called": "expect_tools",
    "tools_not_called": "forbid_tools",
    "backend_matches": "expect_backend",
    "output_contains": "must_contain",
    "output_excludes": "must_not_contain",
    "search_privacy": "private_terms",
    "goal": "expected",
}
FILES = sorted(EVALS.glob("*.yaml"))
NEW = [f for f in FILES if int(f.name[:2]) >= 8]


def load(path):
    """Return the single test in an eval file."""
    data = yaml.safe_load(path.read_text())
    assert isinstance(data, list) and len(data) == 1, "one test per file"
    return data[0]


def grader_names(test):
    """Return the grader function names a test asserts, in order."""
    names = []
    for item in test["assert"]:
        assert item["type"] == "python"
        prefix, _, name = item["value"].rpartition(":")
        assert prefix == "file://graders.py", item["value"]
        names.append(name)
    return names


def test_provider_config_keeps_list_vars_whole():
    """Without this promptfoo runs one test per string in followups, expect_tools and the rest."""
    config = yaml.safe_load((TESTS / "promptfooconfig.yaml").read_text())
    assert config["defaultTest"]["options"]["disableVarExpansion"] is True


def test_file_names_follow_the_convention_and_are_unique():
    """NN_<agent>_<name>.yaml with a unique number and a unique name."""
    stems = [f.name for f in FILES]
    assert all(NAME.match(n) for n in stems), [n for n in stems if not NAME.match(n)]
    assert len({n[:2] for n in stems}) == len(stems), "numbers must be unique"
    assert len({n[3:] for n in stems}) == len(stems), "names must be unique"


@pytest.mark.parametrize("path", FILES, ids=lambda p: p.name)
def test_every_eval_parses_and_matches_its_file_name(path):
    """The agent in the name is the agent in the vars, and the vars are valid."""
    test = load(path)
    assert test["description"].strip() and test["assert"]
    case = parse(test["vars"])
    assert case.agent == NAME.match(path.name).group(2)


@pytest.mark.parametrize("path", FILES, ids=lambda p: p.name)
def test_every_assertion_is_a_real_grader_with_its_input(path):
    """A grader without its var raises mid-run; a var without its grader checks nothing."""
    test = load(path)
    names = grader_names(test)
    for name in names:
        assert callable(getattr(graders, name, None)), f"unknown grader {name}"
    for name in names:
        if name in NEEDS:
            assert test["vars"].get(NEEDS[name]), f"{name} needs vars.{NEEDS[name]}"
    for grader, var in NEEDS.items():
        if test["vars"].get(var) and var != "expected":
            assert grader in names, f"vars.{var} is set but {grader} is not asserted"
    assert len(names) == len(set(names)), "a grader is asserted twice"


@pytest.mark.parametrize("path", NEW, ids=lambda p: p.name)
def test_account_names_exist_for_the_persona(path):
    """A typo in an account name would only surface during a paid run."""
    test = load(path)["vars"]
    known = ACCOUNTS[test["persona"]]
    used = [g["account"] for g in test.get("goals", [])]
    used += [r["account"] for r in test.get("rules", []) if r.get("account")]
    used += [t["account"] for t in test.get("seed_transactions", [])]
    assert set(used) <= known, sorted(set(used) - known)
    for budget in test.get("budgets", []):
        assert budget["category"] in CATEGORIES


@pytest.mark.parametrize("path", NEW, ids=lambda p: p.name)
def test_new_evals_use_the_two_documented_personas(path):
    """Only personas whose accounts are listed in this file are used by the new evals."""
    assert load(path)["vars"]["persona"] in ACCOUNTS


def test_the_suite_has_the_planned_breadth():
    """At least 43 new evals, using every deterministic grader and every chat-only feature."""
    tests = [load(f)["vars"] | {"_graders": grader_names(load(f))} for f in NEW]
    assert len(NEW) >= 43
    used = {g for t in tests for g in t["_graders"]}
    assert used >= {
        "tools_called",
        "tools_not_called",
        "backend_matches",
        "nothing_saved",
        "confirmed_before_write",
        "output_contains",
        "output_excludes",
        "search_privacy",
        "goal",
        "accuracy",
        "adherence",
    }
    assert {t.get("backend", "ok") for t in tests} == {"ok", "conflict", "down"}
    assert any(t.get("followups") for t in tests)
    assert any(t.get("seed_transactions") for t in tests)
    assert any(t.get("search_results") is not None for t in tests)
    assert any(t.get("search_results") == [] for t in tests)
    assert {"christian@test.com", "arthur@test.com"} <= {t["persona"] for t in tests}
