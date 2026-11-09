"""Load-context node: gather the user's accounts, rules, budgets and goals for the prompt."""

import logging
from collections.abc import Callable
from datetime import date

from agent.logging import NODE, traced_node
from agent.nodes.context import (
    format_accounts_context,
    format_budgets,
    format_goals,
    format_rules,
)
from agent.state import AnalyzerState, StateUpdate
from db import load_accounts, load_budgets, load_goals, load_rules

_log = logging.getLogger(NODE)


def _section(name: str, build: Callable[[], str]) -> str:
    """Build one context section; a failure leaves a placeholder instead of aborting the run."""
    try:
        return build()
    except Exception as exc:
        _log.warning("load_context.failed section=%s type=%s", name, type(exc).__name__)
        _log.debug("load_context.failed detail", exc_info=True)
        return f"{name.capitalize()} unavailable."


@traced_node("load_context")
def load_account_context(state: AnalyzerState) -> StateUpdate:
    """
    Load the accounts, rules, budgets and goals as text. No LLM involved.

    Every run gets all four, so the model cannot skip one. The transaction is already claimed
    at this point, so a failure while loading must not abort the run (that would leave the claim
    stuck in progress): a failed section is replaced by a placeholder and the analysis continues.
    """
    user_id, txn = state["user_id"], state["transaction"]
    today = date.today()
    sections = [
        _section(
            "accounts",
            lambda: format_accounts_context(today, load_accounts(user_id), txn.account_id),
        ),
        _section("rules", lambda: format_rules(load_rules(user_id, txn.account_id))),
        _section("budgets", lambda: format_budgets(today, load_budgets(user_id, today))),
        _section("goals", lambda: format_goals(load_goals(user_id))),
    ]
    return {"context": "\n\n".join(sections)}
