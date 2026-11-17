"""Everything the tools share, so the tool files hold only tools."""

import functools
import logging
import math
import re
from collections.abc import Callable
from datetime import date, timedelta
from typing import Annotated, Any, Literal

from langgraph.prebuilt import InjectedState

from agent.formatting import money
from agent.logging import TOOL
from agent.safety import sanitize
from db import Account, Goal, load_accounts, load_all_rules, load_budgets, load_goals

MAX_TOOL_CHARS = 4000
MAX_AMOUNT = 1_000_000_000
MAX_MONTHLY_LIMIT = 1_000_000
LABEL_MAX = 60
NAME_MAX = 100
RULE_MAX = 500
DESCRIPTION_MAX = 500
UNAVAILABLE = "That data is temporarily unavailable. Tell the user, and do not guess."

State = Annotated[dict[str, Any], InjectedState]
"""The graph state a tool receives. The model never sees or supplies it."""

Kind = Literal["budget", "rule", "goal"]
ROUTES = {"budget": "/api/budgets", "rule": "/api/rules", "goal": "/api/goals"}
UPDATABLE = {
    "budget": {"name", "monthly_limit"},
    "rule": {"description", "account_names"},
    "goal": {"name", "description", "target_amount", "target_date"},
}
STAGED = (
    "Proposed, NOT saved yet (reference {ref}): {preview}. Show this to the user and ask them to "
    "confirm. Only after they agree in their next message, call confirm_action."
)
BAD_ROUTE = "That change is not one the assistant is allowed to make."

_ALLOWED_ROUTE = re.compile(r"/api/(?:budgets|rules|goals)(?P<item>/[0-9]+)?")
_log = logging.getLogger(TOOL)


class Refusal(Exception):  # noqa: N818 - a normal tool outcome, not an error
    """A tool declines a request. The message becomes the tool's result for the model."""


# --- output -------------------------------------------------------------------------------


def today() -> date:
    """Return today's date (tools call this so tests can pin it)."""
    return date.today()


def cap(text: str) -> str:
    """Cut tool output to MAX_TOOL_CHARS so one tool cannot flood the prompt."""
    if len(text) <= MAX_TOOL_CHARS:
        return text
    return text[:MAX_TOOL_CHARS] + "\n... (truncated)"


def label(text: str | None) -> str:
    """Make a name or description from the user or the bank safe to show the model."""
    return sanitize(text or "", LABEL_MAX)[0]


def safe_tool(fn: Callable[..., str]) -> Callable[..., str]:
    """
    Make a tool body safe to expose to the model.

    A `Refusal` returns its own message. Any other error returns a generic message and logs only
    the exception type, so database or network details never reach the model or the user.
    Whatever is returned is cut to MAX_TOOL_CHARS.
    """

    @functools.wraps(fn)
    def wrapper(*args: Any, **kwargs: Any) -> str:
        try:
            return cap(fn(*args, **kwargs))
        except Refusal as refusal:
            return cap(str(refusal))
        except Exception as exc:
            _log.warning("tool.failed name=%s type=%s", fn.__name__, type(exc).__name__)
            _log.debug("tool.failed detail", exc_info=True)
            return UNAVAILABLE

    return wrapper


# --- calculations -------------------------------------------------------------------------


def window(days: int) -> tuple[int, date, date]:
    """Return (days, start, end) for the last `days` days including today; end is exclusive."""
    days = max(1, min(days, 365))
    end = today() + timedelta(days=1)
    return days, end - timedelta(days=days), end


def goal_balance(goal: Goal) -> float:
    """Return the goal account's current balance, or its starting balance if unknown."""
    return goal.baseline_balance if goal.current_balance is None else goal.current_balance


def spendable(account: Account) -> float:
    """Return what can be spent from an account: its available balance, else its current one."""
    if account.available_balance is not None:
        return account.available_balance
    return account.current_balance or 0.0


# --- checking what the model asked for ----------------------------------------------------


def route_allowed(method: str, path: str) -> bool:
    """Allow only POST on a collection and PUT/DELETE on one item of budgets, rules or goals."""
    match = _ALLOWED_ROUTE.fullmatch(path)
    if match is None:
        return False
    if match.group("item"):
        return method in ("PUT", "DELETE")
    return method == "POST"


def clean_text(value: str, what: str, max_len: int) -> str:
    """Trim `value` and require 1..max_len characters."""
    clean = value.strip()
    if not 1 <= len(clean) <= max_len:
        raise Refusal(f"The {what} must be 1-{max_len} characters.")
    return clean


def check_limit(value: float) -> float:
    """Require a monthly limit above zero and at most MAX_MONTHLY_LIMIT."""
    if not math.isfinite(value) or not 0 < value <= MAX_MONTHLY_LIMIT:
        raise Refusal(
            f"The monthly limit must be more than $0 and at most {money(MAX_MONTHLY_LIMIT)}."
        )
    return value


def check_target(value: float) -> float:
    """Require a finite savings target above zero and at most MAX_AMOUNT."""
    if not math.isfinite(value) or not 0 < value <= MAX_AMOUNT:
        raise Refusal(f"The target amount must be greater than 0 and at most {money(MAX_AMOUNT)}.")
    return value


def clean_description(value: str) -> str:
    """Trim a goal description and require it to fit DESCRIPTION_MAX."""
    clean = value.strip()
    if len(clean) > DESCRIPTION_MAX:
        raise Refusal(f"The description must be at most {DESCRIPTION_MAX} characters.")
    return clean


def future_date(value: str) -> date:
    """Parse YYYY-MM-DD and require a date after today."""
    try:
        parsed = date.fromisoformat(value.strip())
    except ValueError:
        raise Refusal("The target date must look like YYYY-MM-DD.") from None
    if parsed <= today():
        raise Refusal("The target date must be in the future.")
    return parsed


# --- looking things up in the user's own data ---------------------------------------------


def pick_account(accounts: list[Account], wanted: str, offer: str) -> Account:
    """Find one account by exact name, else by a unique case-insensitive substring."""
    key = wanted.strip().lower()
    exact = [a for a in accounts if a.name.lower() == key]
    matches = exact or [a for a in accounts if key and key in a.name.lower()]
    if len(matches) == 1:
        return matches[0]
    names = ", ".join(label(a.name) for a in matches or accounts) or "none"
    if not matches:
        raise Refusal(f"No account matches {label(wanted)!r}. {offer} accounts: {names}.")
    raise Refusal(f"{label(wanted)!r} matches several accounts: {names}. Ask the user which one.")


def accounts_named(uid: int, names: list[str]) -> list[Account]:
    """Resolve account names (any type) to the user's accounts, without duplicates."""
    accounts = load_accounts(uid)
    found: dict[str, Account] = {}
    for name in names:
        match = pick_account(accounts, name, "The user's")
        found[match.account_id] = match
    return list(found.values())


def depository_accounts(uid: int) -> list[Account]:
    """Return the user's checking and savings accounts."""
    return [a for a in load_accounts(uid) if a.account_type == "depository"]


def scope(accounts: list[Account]) -> str:
    """Describe which accounts a rule covers."""
    return ", ".join(label(a.name) for a in accounts) or "all accounts"


def budget_in(uid: int, category: str):
    """Return the user's budget for a category, or None."""
    return next((b for b in load_budgets(uid, today()) if b.category == category), None)


def find(kind: Kind, uid: int, item_id: int) -> tuple[Any, str]:
    """Return the user's item and a short label for it, or refuse."""
    if kind == "budget":
        found = next((i for i in load_budgets(uid, today()) if i.id == item_id), None)
        name = found.name if found else ""
    elif kind == "rule":
        found = next((i for i in load_all_rules(uid) if i.id == item_id), None)
        name = found.description if found else ""
    else:
        found = next((i for i in load_goals(uid) if i.id == item_id), None)
        name = found.name if found else ""
    if found is None:
        raise Refusal(f"No {kind} with id {item_id}. Look it up with list_{kind}s first.")
    return found, label(name)
