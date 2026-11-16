"""Parse and validate the `vars` of one eval test into typed definitions."""

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures" / "sandbox-users.json"
AGENTS = ("analyze", "chat")
# Mirrors migrations/009 and agent.data_models.Category.
CATEGORIES = (
    "SHOPPING",
    "ENTERTAINMENT",
    "UTILITIES",
    "HEALTHCARE",
    "FOOD_DINING",
    "GROCERIES",
    "TRANSPORTATION",
    "OTHER",
)
BACKEND_MODES = ("ok", "conflict", "down")
METHODS = ("POST", "PUT", "DELETE")
# Every tool ChatAgent registers. A unit test keeps this in step with the agent.
KNOWN_TOOLS = (
    "web_search",
    "list_accounts",
    "list_transactions",
    "list_budgets",
    "list_goals",
    "list_rules",
    "spending_by_category",
    "spending_by_merchant",
    "compare_periods",
    "budget_status",
    "goal_progress",
    "find_recurring_charges",
    "can_i_afford",
    "propose_budget",
    "propose_rule",
    "propose_goal",
    "propose_update",
    "propose_delete",
    "confirm_action",
)
# Vars that only make sense for the chat agent.
CHAT_ONLY = (
    "followups",
    "backend",
    "seed_transactions",
    "search_results",
    "expect_tools",
    "forbid_tools",
    "expect_backend",
    "private_terms",
)


class DefinitionError(ValueError):
    """An eval test definition is invalid."""


@dataclass(frozen=True)
class GoalDef:
    """A savings goal to insert for the persona."""

    name: str
    description: str | None
    account: str
    target_amount: float
    days_until_target: int | None


@dataclass(frozen=True)
class BudgetDef:
    """A monthly category budget to insert for the persona."""

    name: str
    category: str
    monthly_limit: float


@dataclass(frozen=True)
class RuleDef:
    """A natural-language rule; `account` is an account name, or None for every account."""

    description: str
    account: str | None


@dataclass(frozen=True)
class TransactionDef:
    """The transaction the analyzer is asked to review (not stored in the database)."""

    name: str
    amount: float
    account: str
    category: str
    days_ago: int


@dataclass(frozen=True)
class SearchResultDef:
    """One stubbed web search result."""

    title: str
    url: str
    content: str


@dataclass(frozen=True)
class EvalCase:
    """One validated eval test."""

    persona: dict[str, Any]
    agent: str
    goals: tuple[GoalDef, ...]
    budgets: tuple[BudgetDef, ...]
    rules: tuple[RuleDef, ...]
    transaction: TransactionDef | None
    message: str | None
    expected: str | None
    followups: tuple[str, ...] = ()
    backend: str = "ok"
    seed_transactions: tuple[TransactionDef, ...] = ()
    search_results: tuple[SearchResultDef, ...] | None = None


def load_personas(path: Path = FIXTURES) -> dict[str, dict[str, Any]]:
    """Load the sandbox users, keyed by email."""
    return {user["email"]: user for user in json.loads(path.read_text())}


def _get(item: Any, key: str, where: str) -> Any:
    """Return `item[key]`, or raise if it is missing or empty."""
    if not isinstance(item, dict) or item.get(key) in (None, ""):
        raise DefinitionError(f"{where}: `{key}` is required")
    return item[key]


def _number(value: Any, where: str, key: str) -> float:
    """Convert `value` to a float or raise."""
    try:
        return float(value)
    except (TypeError, ValueError):
        raise DefinitionError(f"{where}: `{key}` must be a number") from None


def _positive(item: Any, key: str, where: str) -> float:
    """Return `item[key]` as a number greater than zero."""
    number = _number(_get(item, key, where), where, key)
    if number <= 0:
        raise DefinitionError(f"{where}: `{key}` must be greater than 0")
    return number


def _category(item: dict[str, Any], where: str, default: str | None = None) -> str:
    """Return a valid spending category."""
    category = item.get("category", default)
    if category not in CATEGORIES:
        raise DefinitionError(f"{where}: category must be one of {', '.join(CATEGORIES)}")
    return category


def _goal(item: Any, index: int) -> GoalDef:
    where = f"goals[{index}]"
    days = item.get("days_until_target")
    return GoalDef(
        name=_get(item, "name", where),
        description=item.get("description"),
        account=_get(item, "account", where),
        target_amount=_positive(item, "target_amount", where),
        days_until_target=None if days is None else int(_number(days, where, "days_until_target")),
    )


def _budget(item: Any, index: int) -> BudgetDef:
    where = f"budgets[{index}]"
    return BudgetDef(
        name=_get(item, "name", where),
        category=_category(item, where),
        monthly_limit=_positive(item, "monthly_limit", where),
    )


def _rule(item: Any, index: int) -> RuleDef:
    where = f"rules[{index}]"
    return RuleDef(description=_get(item, "description", where), account=item.get("account"))


def _transaction(item: Any, where: str = "transaction") -> TransactionDef:
    return TransactionDef(
        name=_get(item, "name", where),
        amount=_number(_get(item, "amount", where), where, "amount"),
        account=_get(item, "account", where),
        category=_category(item, where, default="OTHER"),
        days_ago=int(_number(item.get("days_ago", 0), where, "days_ago")),
    )


def _strings(test_vars: dict[str, Any], key: str) -> tuple[str, ...]:
    """Return `test_vars[key]` as a tuple of non-empty strings (missing means empty)."""
    value = test_vars.get(key)
    if value is None:
        return ()
    if not isinstance(value, list) or not all(isinstance(v, str) and v for v in value):
        raise DefinitionError(f"{key} must be a list of non-empty strings")
    return tuple(value)


def _tools(test_vars: dict[str, Any], key: str) -> None:
    """Check that every name in `test_vars[key]` is a real chat tool."""
    unknown = [name for name in _strings(test_vars, key) if name not in KNOWN_TOOLS]
    if unknown:
        raise DefinitionError(
            f"{key}: unknown tool {unknown}; choose from {', '.join(KNOWN_TOOLS)}"
        )


def _search_result(item: Any, index: int) -> SearchResultDef:
    where = f"search_results[{index}]"
    return SearchResultDef(
        title=_get(item, "title", where),
        url=item.get("url", "https://example.test"),
        content=_get(item, "content", where),
    )


def _expect_backend(item: Any, index: int) -> None:
    """Validate one expected backend call."""
    where = f"expect_backend[{index}]"
    if not isinstance(item, dict):
        raise DefinitionError(f"{where}: must be a mapping")
    if item.get("method") not in METHODS:
        raise DefinitionError(f"{where}: method must be one of {', '.join(METHODS)}")
    if (item.get("path") is None) == (item.get("path_prefix") is None):
        raise DefinitionError(f"{where}: give exactly one of `path` or `path_prefix`")
    if item.get("body") is not None and not isinstance(item["body"], dict):
        raise DefinitionError(f"{where}: `body` must be a mapping")
    if "turn" in item and (isinstance(item["turn"], bool) or not isinstance(item["turn"], int)):
        raise DefinitionError(f"{where}: `turn` must be a whole number")


def _validate_assertion_inputs(test_vars: dict[str, Any]) -> None:
    """Check the vars that graders.py reads straight from the test (they are not stored)."""
    _tools(test_vars, "expect_tools")
    _tools(test_vars, "forbid_tools")
    for index, item in enumerate(test_vars.get("expect_backend") or []):
        _expect_backend(item, index)
    for key in ("must_contain", "must_not_contain", "private_terms"):
        _strings(test_vars, key)


def parse(test_vars: dict[str, Any], personas: dict[str, dict[str, Any]] | None = None) -> EvalCase:
    """Validate a test's `vars` and return it as an `EvalCase`. Raises `DefinitionError`."""
    personas = load_personas() if personas is None else personas
    email = _get(test_vars, "persona", "vars")
    if email not in personas:
        raise DefinitionError(f"unknown persona {email!r}; choose one of {sorted(personas)}")
    agent = _get(test_vars, "agent", "vars")
    if agent not in AGENTS:
        raise DefinitionError(f"agent must be one of {', '.join(AGENTS)}, got {agent!r}")

    budgets = tuple(_budget(b, i) for i, b in enumerate(test_vars.get("budgets") or []))
    categories = [b.category for b in budgets]
    if len(categories) != len(set(categories)):
        raise DefinitionError("budgets: one budget per category")

    transaction = message = None
    chat_extras: dict[str, Any] = {}
    if agent == "analyze":
        misplaced = [key for key in CHAT_ONLY if test_vars.get(key) not in (None, [], "")]
        if "search_results" in test_vars:
            misplaced.append("search_results")
        if misplaced:
            raise DefinitionError(f"{misplaced[0]} is only for the chat agent")
        transaction = _transaction(_get(test_vars, "transaction", "vars"))
    else:
        message = _get(test_vars, "message", "vars")
        backend = test_vars.get("backend", "ok")
        if backend not in BACKEND_MODES:
            raise DefinitionError(f"backend must be one of {', '.join(BACKEND_MODES)}")
        stub = test_vars.get("search_results")
        chat_extras = {
            "followups": _strings(test_vars, "followups"),
            "backend": backend,
            "seed_transactions": tuple(
                _transaction(t, f"seed_transactions[{i}]")
                for i, t in enumerate(test_vars.get("seed_transactions") or [])
            ),
            "search_results": None
            if stub is None
            else tuple(_search_result(r, i) for i, r in enumerate(stub)),
        }
        _validate_assertion_inputs(test_vars)

    return EvalCase(
        persona=personas[email],
        agent=agent,
        goals=tuple(_goal(g, i) for i, g in enumerate(test_vars.get("goals") or [])),
        budgets=budgets,
        rules=tuple(_rule(r, i) for i, r in enumerate(test_vars.get("rules") or [])),
        transaction=transaction,
        message=message,
        expected=test_vars.get("expected"),
        **chat_extras,
    )
