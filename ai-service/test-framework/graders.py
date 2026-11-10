"""
promptfoo assertions: each one asks the judge LLM to grade the agent's response.

Use as `value: file://graders.py:adherence` (or accuracy / goal) with an optional
`config: {threshold: 0.7}` on the assertion.
"""

import json

import judge

DEFAULT_THRESHOLD = 0.7

ADHERENCE = (
    "Adherence. Using the user's rules, budgets and goals in <facts>, decide whether the "
    "agent's response respects them: it must notice and correctly apply every rule, budget "
    "and goal relevant to the request, and must not contradict or ignore any."
)
ACCURACY = (
    "Accuracy. Check every concrete claim in the agent's response (amounts, totals, remaining "
    "budget, merchants, dates, account names) against <facts>. Penalize anything wrong, "
    "invented or unsupported by the facts."
)
GOAL = (
    "Goal. Decide whether the agent's response achieves what <expected_outcome> says it "
    "should, for the request in <user_request>."
)


def _vars(context: dict) -> dict:
    return context.get("vars") or {}


def _facts(context: dict) -> dict:
    facts = ((context.get("providerResponse") or {}).get("metadata") or {}).get("facts")
    if not facts:
        raise ValueError("the provider returned no facts; cannot grade")
    return facts


def _request(test_vars: dict) -> str:
    """Return the prompt the agent was given: a transaction (analyze) or a message (chat)."""
    if test_vars.get("agent") == "chat":
        return test_vars.get("message", "")
    return json.dumps(test_vars.get("transaction"), indent=2)


def _tag(name: str, body: str) -> str:
    return f"<{name}>\n{body}\n</{name}>"


def _grade(rubric: str, output: str, context: dict, sections: list[tuple[str, str]]) -> dict:
    """Build the judge's material, score it and apply the threshold."""
    threshold = float((context.get("config") or {}).get("threshold", DEFAULT_THRESHOLD))
    material = "\n\n".join(
        [
            _tag("user_request", _request(_vars(context))),
            *(_tag(name, body) for name, body in sections),
            _tag("agent_output", output),
        ]
    )
    verdict = judge.judge(rubric, material)
    return {"pass": verdict.score >= threshold, "score": verdict.score, "reason": verdict.reason}


def adherence(output: str, context: dict) -> dict:
    """Grade whether the response respects the seeded rules, budgets and goals."""
    return _grade(ADHERENCE, output, context, [("facts", json.dumps(_facts(context), indent=2))])


def accuracy(output: str, context: dict) -> dict:
    """Grade whether the numbers and facts in the response match the database."""
    return _grade(ACCURACY, output, context, [("facts", json.dumps(_facts(context), indent=2))])


def goal(output: str, context: dict) -> dict:
    """Grade whether the response achieves the outcome the test author expects."""
    expected = _vars(context).get("expected")
    if not expected:
        raise ValueError("the goal grader needs `expected` in the test's vars")
    return _grade(GOAL, output, context, [("expected_outcome", expected)])
