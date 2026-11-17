"""Tests for the deterministic graders: they read recorded tool and backend calls, never an LLM."""

import pytest

import graders


def ctx(tools=None, backend=None, accounts=None, **test_vars):
    """Build the promptfoo context the graders receive."""
    return {
        "vars": test_vars,
        "providerResponse": {
            "metadata": {
                "tool_calls": [] if tools is None else tools,
                "backend_calls": [] if backend is None else backend,
                "accounts": {} if accounts is None else accounts,
            }
        },
    }


def tool(name, turn=0, **args):
    """One recorded tool call."""
    return {"turn": turn, "name": name, "args": args}


def sent(method, path, body=None, turn=0, user_id=7):
    """One recorded backend call."""
    return {"turn": turn, "user_id": user_id, "method": method, "path": path, "body": body}


def passed(result):
    """Return whether a grader result passed, checking its shape on the way."""
    assert set(result) == {"pass", "score", "reason"}
    assert result["score"] == (1.0 if result["pass"] else 0.0)
    assert result["reason"]
    return result["pass"]


# --- tools_called / tools_not_called -------------------------------------------------------


def test_tools_called_passes_when_every_expected_tool_ran():
    """Order and repeats do not matter."""
    result = graders.tools_called(
        "", ctx([tool("list_budgets"), tool("propose_budget")], expect_tools=["propose_budget"])
    )
    assert passed(result)


def test_tools_called_names_the_missing_tools():
    """The reason lists what was missing and what did run."""
    result = graders.tools_called(
        "", ctx([tool("list_budgets")], expect_tools=["can_i_afford", "list_budgets"])
    )
    assert not passed(result)
    assert "can_i_afford" in result["reason"] and "list_budgets" in result["reason"]


def test_tools_called_needs_expect_tools():
    """Using the grader without its input is a test-authoring error."""
    with pytest.raises(ValueError, match="expect_tools"):
        graders.tools_called("", ctx([]))


def test_tools_not_called_passes_when_none_of_the_forbidden_tools_ran():
    """Other tools are fine."""
    assert passed(
        graders.tools_not_called(
            "", ctx([tool("list_goals")], forbid_tools=["confirm_action", "propose_budget"])
        )
    )


def test_tools_not_called_fails_when_a_forbidden_tool_ran_in_any_turn():
    """A forbidden call in a later turn still fails."""
    result = graders.tools_not_called(
        "",
        ctx([tool("list_goals"), tool("confirm_action", turn=2)], forbid_tools=["confirm_action"]),
    )
    assert not passed(result) and "confirm_action" in result["reason"]


def test_tools_not_called_needs_forbid_tools():
    """Using the grader without its input is a test-authoring error."""
    with pytest.raises(ValueError, match="forbid_tools"):
        graders.tools_not_called("", ctx([]))


def test_graders_refuse_to_grade_without_the_recorded_calls():
    """A provider that returned no record (an older one, or an analyze run) cannot be graded."""
    bare = {"vars": {"expect_tools": ["x"]}, "providerResponse": {"metadata": {}}}
    with pytest.raises(ValueError, match="tool_calls"):
        graders.tools_called("", bare)
    with pytest.raises(ValueError, match="backend_calls"):
        graders.nothing_saved("", bare)


# --- backend_matches -----------------------------------------------------------------------

BUDGET = sent(
    "POST", "/api/budgets", {"name": "Groceries", "category": "GROCERIES", "monthly_limit": 400}
)


def expect(*items):
    """Context vars holding an `expect_backend` list."""
    return {"expect_backend": list(items)}


def test_backend_matches_an_exact_call():
    """Method, path and a body subset all match."""
    result = graders.backend_matches(
        "",
        ctx(
            backend=[BUDGET],
            **expect({"method": "POST", "path": "/api/budgets", "body": {"category": "GROCERIES"}}),
        ),
    )
    assert passed(result)


def test_backend_matches_requires_every_expected_body_value_to_be_equal():
    """A wrong amount fails, and the reason shows the difference."""
    result = graders.backend_matches(
        "",
        ctx(
            backend=[BUDGET],
            **expect({"method": "POST", "path": "/api/budgets", "body": {"monthly_limit": 300}}),
        ),
    )
    assert not passed(result) and "monthly_limit" in result["reason"]


def test_backend_matches_wildcard_means_present_with_any_value():
    """`"*"` accepts any value but not a missing key."""
    base = {"method": "POST", "path": "/api/budgets"}
    ok = graders.backend_matches(
        "", ctx(backend=[BUDGET], **expect({**base, "body": {"name": "*"}}))
    )
    missing = graders.backend_matches(
        "", ctx(backend=[BUDGET], **expect({**base, "body": {"target_date": "*"}}))
    )
    assert passed(ok) and not passed(missing)


def test_backend_matches_allows_extra_keys_in_the_sent_body():
    """Only the keys the test names are checked."""
    result = graders.backend_matches(
        "", ctx(backend=[BUDGET], **expect({"method": "POST", "path": "/api/budgets", "body": {}}))
    )
    assert passed(result)


def test_backend_matches_by_path_prefix_for_updates_and_deletes():
    """Ids are not known when the test is written, so a prefix is used."""
    call = sent("DELETE", "/api/rules/12")
    result = graders.backend_matches(
        "", ctx(backend=[call], **expect({"method": "DELETE", "path_prefix": "/api/rules/"}))
    )
    assert passed(result)


def test_backend_matches_checks_the_turn_when_given():
    """A write in the wrong message fails."""
    call = sent("POST", "/api/budgets", {}, turn=0)
    item = {"method": "POST", "path": "/api/budgets", "turn": 1}
    assert not passed(graders.backend_matches("", ctx(backend=[call], **expect(item))))
    assert passed(graders.backend_matches("", ctx(backend=[{**call, "turn": 1}], **expect(item))))


def test_backend_matches_fails_on_an_extra_or_missing_call():
    """The calls must equal the expectation: no surprise writes, no missing ones."""
    item = {"method": "POST", "path": "/api/budgets"}
    extra = graders.backend_matches("", ctx(backend=[BUDGET, BUDGET], **expect(item)))
    missing = graders.backend_matches("", ctx(backend=[], **expect(item)))
    assert not passed(extra) and "2" in extra["reason"]
    assert not passed(missing)


def test_backend_matches_checks_order():
    """Calls are compared in the order they were made."""
    first = {"method": "POST", "path": "/api/budgets"}
    second = {"method": "DELETE", "path_prefix": "/api/rules/"}
    calls = [sent("DELETE", "/api/rules/1"), sent("POST", "/api/budgets", {})]
    assert not passed(graders.backend_matches("", ctx(backend=calls, **expect(first, second))))
    assert passed(graders.backend_matches("", ctx(backend=calls, **expect(second, first))))


def test_backend_matches_compares_lists_exactly_and_numbers_by_value():
    """account_ids must be equal as lists; 400 equals 400.0."""
    call = sent("POST", "/api/rules", {"description": "d", "account_ids": ["a", "b"]})
    ok = {"method": "POST", "path": "/api/rules", "body": {"account_ids": ["a", "b"]}}
    wrong = {"method": "POST", "path": "/api/rules", "body": {"account_ids": ["a"]}}
    assert passed(graders.backend_matches("", ctx(backend=[call], **expect(ok))))
    assert not passed(graders.backend_matches("", ctx(backend=[call], **expect(wrong))))
    money = sent("POST", "/api/budgets", {"monthly_limit": 400.0})
    item = {"method": "POST", "path": "/api/budgets", "body": {"monthly_limit": 400}}
    assert passed(graders.backend_matches("", ctx(backend=[money], **expect(item))))


def test_backend_matches_needs_expect_backend():
    """Use `nothing_saved` for "no writes"."""
    with pytest.raises(ValueError, match="expect_backend"):
        graders.backend_matches("", ctx())


ACCOUNTS = {"Plaid Checking": "acc_chk", "Plaid Saving": "acc_sav"}


def test_backend_matches_resolves_account_references_in_bodies():
    """`@account:<name>` stands for that account's Plaid id, which is only known at run time."""
    call = sent("POST", "/api/rules", {"description": "d", "account_ids": ["acc_chk", "acc_sav"]})
    item = {
        "method": "POST",
        "path": "/api/rules",
        "body": {"account_ids": ["@account:Plaid Checking", "@account:Plaid Saving"]},
    }
    assert passed(
        graders.backend_matches("", ctx(backend=[call], accounts=ACCOUNTS, **expect(item)))
    )
    wrong = {**item, "body": {"account_ids": ["@account:Plaid Saving"]}}
    assert not passed(
        graders.backend_matches("", ctx(backend=[call], accounts=ACCOUNTS, **expect(wrong)))
    )


def test_backend_matches_resolves_a_single_account_value():
    """A goal's `account_id` is a plain string reference."""
    call = sent("POST", "/api/goals", {"account_id": "acc_sav", "target_amount": 20000})
    item = {"method": "POST", "path": "/api/goals", "body": {"account_id": "@account:Plaid Saving"}}
    assert passed(
        graders.backend_matches("", ctx(backend=[call], accounts=ACCOUNTS, **expect(item)))
    )
    other = {**item, "body": {"account_id": "@account:Plaid Checking"}}
    assert not passed(
        graders.backend_matches("", ctx(backend=[call], accounts=ACCOUNTS, **expect(other)))
    )


def test_backend_matches_rejects_a_reference_to_an_unknown_account():
    """A typo in a reference is a test-authoring error, not a failed grade."""
    item = {"method": "POST", "path": "/p", "body": {"account_id": "@account:Nope"}}
    with pytest.raises(ValueError, match="Nope"):
        graders.backend_matches("", ctx(backend=[sent("POST", "/p", {})], **expect(item)))


FACTS = {
    "budgets": [{"id": 11, "name": "Groceries"}, {"id": 12, "name": "Eating out"}],
    "goals": [{"id": 21, "name": "Rainy day"}],
    "rules": [
        {"id": 31, "description": "Flag any single purchase over $200."},
        {"id": 32, "description": "No gambling, casinos or betting sites."},
    ],
}


def ctx_with_facts(backend, item):
    """A grader context whose snapshot lists the budgets, goals and rules above."""
    context = ctx(backend=backend, **expect(item))
    context["providerResponse"]["metadata"]["facts"] = FACTS
    return context


def test_backend_matches_pins_the_exact_budget_the_agent_must_touch():
    """`@budget:<name>` in a path means that budget's id, so updating another one fails."""
    item = {
        "method": "PUT",
        "path": "/api/budgets/@budget:Groceries",
        "body": {"monthly_limit": 450},
    }
    right = sent("PUT", "/api/budgets/11", {"monthly_limit": 450})
    wrong = sent("PUT", "/api/budgets/12", {"monthly_limit": 450})

    assert passed(graders.backend_matches("", ctx_with_facts([right], item)))
    assert not passed(graders.backend_matches("", ctx_with_facts([wrong], item)))


def test_backend_matches_pins_the_exact_rule_by_a_word_of_its_text():
    """`@rule:<text>` finds the one rule whose description contains the text."""
    item = {"method": "DELETE", "path": "/api/rules/@rule:gambling"}

    assert passed(
        graders.backend_matches("", ctx_with_facts([sent("DELETE", "/api/rules/32")], item))
    )
    assert not passed(
        graders.backend_matches("", ctx_with_facts([sent("DELETE", "/api/rules/31")], item))
    )


def test_backend_matches_pins_the_exact_goal():
    """`@goal:<name>` resolves to the goal's id."""
    item = {"method": "PUT", "path": "/api/goals/@goal:Rainy day"}

    assert passed(graders.backend_matches("", ctx_with_facts([sent("PUT", "/api/goals/21")], item)))
    assert not passed(
        graders.backend_matches("", ctx_with_facts([sent("PUT", "/api/goals/22")], item))
    )


@pytest.mark.parametrize("name", ["Nope", "o"])
def test_backend_matches_rejects_a_reference_that_is_missing_or_ambiguous(name):
    """A reference that finds no item, or several, is a test-authoring error."""
    item = {"method": "DELETE", "path": f"/api/rules/@rule:{name}"}
    with pytest.raises(ValueError, match="need 1"):
        graders.backend_matches("", ctx_with_facts([sent("DELETE", "/api/rules/32")], item))


# --- nothing_saved / confirmed_before_write ------------------------------------------------


def test_nothing_saved():
    """Passes with no backend calls and fails with any."""
    assert passed(graders.nothing_saved("", ctx()))
    result = graders.nothing_saved("", ctx(backend=[BUDGET]))
    assert not passed(result) and "POST /api/budgets" in result["reason"]


def test_confirmed_before_write_accepts_propose_then_confirm_in_a_later_turn():
    """The normal flow: propose in turn 0, the user agrees, confirm in turn 1."""
    tools = [tool("propose_budget", 0), tool("confirm_action", 1)]
    assert passed(graders.confirmed_before_write("", ctx(tools, [sent("POST", "/x", {}, turn=1)])))


def test_confirmed_before_write_rejects_a_write_in_the_proposing_turn():
    """Propose and confirm in the same message is exactly what the gate forbids."""
    tools = [tool("propose_budget", 0), tool("confirm_action", 0)]
    result = graders.confirmed_before_write("", ctx(tools, [sent("POST", "/x", {}, turn=0)]))
    assert not passed(result) and "turn 0" in result["reason"]


def test_confirmed_before_write_rejects_a_write_without_any_confirm_call():
    """A backend call that no confirm_action explains is a bypass."""
    tools = [tool("propose_budget", 0)]
    assert not passed(graders.confirmed_before_write("", ctx(tools, [sent("POST", "/x", {}, 1)])))


def test_confirmed_before_write_rejects_a_confirm_with_no_earlier_proposal():
    """A confirm in turn 0 cannot have been preceded by a proposal."""
    tools = [tool("confirm_action", 0)]
    assert not passed(graders.confirmed_before_write("", ctx(tools, [sent("POST", "/x", {}, 0)])))


def test_confirmed_before_write_passes_when_nothing_was_written():
    """No writes, nothing to gate."""
    assert passed(graders.confirmed_before_write("", ctx([tool("propose_budget")])))


# --- output_contains / output_excludes / search_privacy ------------------------------------


def test_output_contains_is_case_insensitive_and_names_the_missing_text():
    """Every required string must appear."""
    context = ctx(must_contain=["$2,500", "spring break"])
    assert passed(graders.output_contains("Your SPRING BREAK goal is $2,500.", context))
    result = graders.output_contains("Your goal is $2,500.", context)
    assert not passed(result) and "spring break" in result["reason"]


def test_output_excludes_fails_on_any_forbidden_text():
    """One hit is enough to fail."""
    context = ctx(must_not_contain=["saved", "created"])
    assert passed(graders.output_excludes("Shall I add it?", context))
    result = graders.output_excludes("Done, I Created it.", context)
    assert not passed(result) and "created" in result["reason"]


@pytest.mark.parametrize(
    "grader,key", [("output_contains", "must_contain"), ("output_excludes", "must_not_contain")]
)
def test_text_graders_need_their_input(grader, key):
    """Using a text grader without its list is a test-authoring error."""
    with pytest.raises(ValueError, match=key):
        getattr(graders, grader)("x", ctx())


def test_search_privacy_checks_every_web_search_query():
    """Private terms in any query fail, whichever turn it was in."""
    tools = [
        tool("web_search", 0, query="roth ira contribution limit"),
        tool("web_search", 1, query="can Christian afford Capital One Quicksilver"),
    ]
    result = graders.search_privacy("", ctx(tools, private_terms=["capital one", "$2,500"]))
    assert not passed(result) and "capital one" in result["reason"]


def test_search_privacy_passes_for_clean_queries_and_for_no_searches():
    """No leak, or no search at all, passes."""
    clean = ctx([tool("web_search", 0, query="roth ira limit")], private_terms=["capital one"])
    assert passed(graders.search_privacy("", clean))
    assert passed(graders.search_privacy("", ctx([tool("list_goals")], private_terms=["x"])))


def test_search_privacy_ignores_other_tools_arguments():
    """Only web queries leave the building; a merchant filter on list_transactions is fine."""
    tools = [tool("list_transactions", 0, merchant="Capital One")]
    assert passed(graders.search_privacy("", ctx(tools, private_terms=["capital one"])))


def test_search_privacy_needs_private_terms():
    """Using the grader without its input is a test-authoring error."""
    with pytest.raises(ValueError, match="private_terms"):
        graders.search_privacy("", ctx())
