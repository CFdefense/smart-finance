"""Tools: propose changes to budgets, rules and goals, and carry them out once the user agrees."""

import math
import re
from datetime import date
from typing import Any, Literal

from langchain_core.tools import BaseTool, tool

from agent.backend_api import BackendApi, BackendError
from agent.data_models import Category
from agent.formatting import money
from agent.pending import PendingActionStore, PendingError
from agent.safety import sanitize
from agent.tools.formatting import MAX_AMOUNT, Refusal, State, safe_tool, today
from db import Account, load_accounts, load_all_rules, load_budgets, load_goals

MAX_MONTHLY_LIMIT = 1_000_000
NAME_MAX = 100
RULE_MAX = 500
DESCRIPTION_MAX = 500
_LABEL_MAX = 60

Kind = Literal["budget", "rule", "goal"]
_ROUTES = {"budget": "/api/budgets", "rule": "/api/rules", "goal": "/api/goals"}
_ALLOWED_ROUTE = re.compile(r"/api/(?:budgets|rules|goals)(?P<item>/[0-9]+)?")
_UPDATABLE = {
    "budget": {"name", "monthly_limit"},
    "rule": {"description", "account_names"},
    "goal": {"name", "description", "target_amount", "target_date"},
}
_STAGED = (
    "Proposed, NOT saved yet (reference {ref}): {preview}. Show this to the user and ask them to "
    "confirm. Only after they agree in their next message, call confirm_action."
)
_BAD_ROUTE = "That change is not one the assistant is allowed to make."


def _route_allowed(method: str, path: str) -> bool:
    """Allow only POST on a collection and PUT/DELETE on one item of budgets, rules or goals."""
    match = _ALLOWED_ROUTE.fullmatch(path)
    if match is None:
        return False
    if match.group("item"):
        return method in ("PUT", "DELETE")
    return method == "POST"


def _label(text: str) -> str:
    """Make a name or description safe to show back to the model."""
    return sanitize(text, _LABEL_MAX)[0]


def _text(value: str, what: str, max_len: int) -> str:
    """Trim `value` and require 1..max_len characters."""
    clean = value.strip()
    if not 1 <= len(clean) <= max_len:
        raise Refusal(f"The {what} must be 1-{max_len} characters.")
    return clean


def _limit(value: float) -> float:
    """Require a monthly limit above zero and at most MAX_MONTHLY_LIMIT."""
    if not math.isfinite(value) or not 0 < value <= MAX_MONTHLY_LIMIT:
        raise Refusal(
            f"The monthly limit must be more than $0 and at most {money(MAX_MONTHLY_LIMIT)}."
        )
    return value


def _target(value: float) -> float:
    """Require a finite savings target above zero and at most MAX_AMOUNT."""
    if not math.isfinite(value) or not 0 < value <= MAX_AMOUNT:
        raise Refusal(f"The target amount must be greater than 0 and at most {money(MAX_AMOUNT)}.")
    return value


def _future_date(text: str) -> date:
    """Parse YYYY-MM-DD and require a date after today."""
    try:
        parsed = date.fromisoformat(text.strip())
    except ValueError:
        raise Refusal("The target date must look like YYYY-MM-DD.") from None
    if parsed <= today():
        raise Refusal("The target date must be in the future.")
    return parsed


def _account(accounts: list[Account], wanted: str, offer: str) -> Account:
    """Find one account by exact name, else by a unique case-insensitive substring."""
    key = wanted.strip().lower()
    exact = [a for a in accounts if a.name.lower() == key]
    matches = exact or [a for a in accounts if key and key in a.name.lower()]
    if len(matches) == 1:
        return matches[0]
    names = ", ".join(_label(a.name) for a in matches or accounts) or "none"
    if not matches:
        raise Refusal(f"No account matches {_label(wanted)!r}. {offer} accounts: {names}.")
    raise Refusal(f"{_label(wanted)!r} matches several accounts: {names}. Ask the user which one.")


def _accounts_named(uid: int, names: list[str]) -> list[Account]:
    """Resolve account names (any type) to the user's accounts, without duplicates."""
    accounts = load_accounts(uid)
    found: dict[str, Account] = {}
    for name in names:
        account = _account(accounts, name, "The user's")
        found[account.account_id] = account
    return list(found.values())


def _scope(accounts: list[Account]) -> str:
    """Describe which accounts a rule covers."""
    return ", ".join(_label(a.name) for a in accounts) or "all accounts"


def _find(kind: Kind, uid: int, item_id: int) -> tuple[Any, str]:
    """Return the user's item and a short label for it, or refuse."""
    if kind == "budget":
        found = next((i for i in load_budgets(uid, today()) if i.id == item_id), None)
        label = found.name if found else ""
    elif kind == "rule":
        found = next((i for i in load_all_rules(uid) if i.id == item_id), None)
        label = found.description if found else ""
    else:
        found = next((i for i in load_goals(uid) if i.id == item_id), None)
        label = found.name if found else ""
    if found is None:
        raise Refusal(f"No {kind} with id {item_id}. Look it up with list_{kind}s first.")
    return found, _label(label)


def make_write_tools(store: PendingActionStore, backend: BackendApi) -> list[BaseTool]:
    """Build the propose/confirm tools around one pending-action store and one backend client."""

    def stage(state: dict[str, Any], kind: str, method: str, path: str, body, preview: str) -> str:
        """Stage one action. `preview` must already be built from sanitized parts."""
        action = store.add(
            state["user_id"],
            kind,
            {"method": method, "path": path, "body": body},
            preview,
            state["request_id"],
        )
        return _STAGED.format(ref=action.action_id, preview=preview)

    @tool
    @safe_tool
    def propose_budget(state: State, name: str, category: Category, monthly_limit: float) -> str:
        """
        Propose a new monthly budget for one spending category. It is not saved yet.

        The user must confirm before anything is saved. There is one budget per category.
        """
        clean = _text(name, "budget name", NAME_MAX)
        limit = _limit(monthly_limit)
        existing = next(
            (b for b in load_budgets(state["user_id"], today()) if b.category == category), None
        )
        if existing is not None:
            raise Refusal(
                f"A budget for {category} already exists (id={existing.id}, "
                f"{money(existing.monthly_limit)}). Use propose_update to change it."
            )
        preview = f"Create budget '{_label(clean)}' for {category}: {money(limit)} per month"
        body = {"name": clean, "category": category, "monthly_limit": limit}
        return stage(state, "budget", "POST", "/api/budgets", body, preview)

    @tool
    @safe_tool
    def propose_rule(state: State, description: str, account_names: list[str] | None = None) -> str:
        """
        Propose a new rule written in the user's own words. It is not saved yet.

        Leave `account_names` empty for a rule that covers every account. The user must confirm.
        """
        text = _text(description, "rule description", RULE_MAX)
        accounts = _accounts_named(state["user_id"], account_names or [])
        preview = f"Create rule '{_label(text)}' for {_scope(accounts)}"
        body = {"description": text, "account_ids": [a.account_id for a in accounts]}
        return stage(state, "rule", "POST", "/api/rules", body, preview)

    @tool
    @safe_tool
    def propose_goal(
        state: State,
        name: str,
        account_name: str,
        target_amount: float,
        target_date: str | None = None,
        description: str | None = None,
    ) -> str:
        """
        Propose a new savings goal tracked on a checking or savings account. It is not saved yet.

        `target_date` is YYYY-MM-DD and must be in the future. The user must confirm.
        """
        clean = _text(name, "goal name", NAME_MAX)
        _target(target_amount)
        uid = state["user_id"]
        depository = [a for a in load_accounts(uid) if a.account_type == "depository"]
        if not depository:
            raise Refusal(
                "The user has no checking or savings account linked, so no goal can be set."
            )
        account = _account(depository, account_name, "Goals can use these")
        balance = account.current_balance or 0.0
        if target_amount <= balance:
            raise Refusal(
                f"The target must be more than the current balance {money(balance)} of that account."
            )
        body: dict[str, Any] = {
            "name": clean,
            "account_id": account.account_id,
            "target_amount": target_amount,
        }
        due = ""
        if target_date is not None:
            body["target_date"] = _future_date(target_date).isoformat()
            due = f" by {body['target_date']}"
        if description and description.strip():
            if len(description.strip()) > DESCRIPTION_MAX:
                raise Refusal(f"The description must be at most {DESCRIPTION_MAX} characters.")
            body["description"] = description.strip()
        preview = (
            f"Create goal '{_label(clean)}' on {_label(account.name)}: "
            f"reach {money(target_amount)}{due}"
        )
        return stage(state, "goal", "POST", "/api/goals", body, preview)

    @tool
    @safe_tool
    def propose_update(
        state: State,
        kind: Kind,
        item_id: int,
        name: str | None = None,
        monthly_limit: float | None = None,
        description: str | None = None,
        target_amount: float | None = None,
        target_date: str | None = None,
        account_names: list[str] | None = None,
    ) -> str:
        """
        Propose changing a budget, rule or goal by its id (see the list tools). Not saved yet.

        Budgets can change `name` and `monthly_limit`. Rules can change `description` and
        `account_names` (an empty list means all accounts). Goals can change `name`,
        `description`, `target_amount` and `target_date`. The user must confirm.
        """
        uid = state["user_id"]
        given = {
            key: value
            for key, value in {
                "name": name,
                "monthly_limit": monthly_limit,
                "description": description,
                "target_amount": target_amount,
                "target_date": target_date,
                "account_names": account_names,
            }.items()
            if value is not None
        }
        if not given:
            raise Refusal("Say what to change before proposing an update.")
        extra = sorted(set(given) - _UPDATABLE[kind])
        if extra:
            raise Refusal(
                f"A {kind} cannot change: {', '.join(extra)}. "
                f"It can change: {', '.join(sorted(_UPDATABLE[kind]))}."
            )
        item, label = _find(kind, uid, item_id)
        body: dict[str, Any] = {}
        parts: list[str] = []
        if kind == "budget":
            if name is not None:
                body["name"] = _text(name, "budget name", NAME_MAX)
                parts.append(f"name '{_label(body['name'])}'")
            if monthly_limit is not None:
                body["monthly_limit"] = _limit(monthly_limit)
                parts.append(f"limit {money(item.monthly_limit)} -> {money(monthly_limit)}")
        elif kind == "rule":
            if description is None:
                text = item.description
            else:
                text = _text(description, "rule description", RULE_MAX)
            ids = item.account_ids
            if account_names is not None:
                accounts = _accounts_named(uid, account_names)
                ids = [a.account_id for a in accounts]
                parts.append(f"applies to {_scope(accounts)}")
            if description is not None:
                parts.insert(0, f"text '{_label(text)}'")
            body = {"description": text, "account_ids": ids, "enabled": True}
        else:
            if name is not None:
                body["name"] = _text(name, "goal name", NAME_MAX)
                parts.append(f"name '{_label(body['name'])}'")
            if description is not None:
                if len(description.strip()) > DESCRIPTION_MAX:
                    raise Refusal(f"The description must be at most {DESCRIPTION_MAX} characters.")
                body["description"] = description.strip()
                parts.append("description")
            if target_amount is not None:
                _target(target_amount)
                if target_amount <= item.baseline_balance:
                    raise Refusal(
                        "The target must be more than the starting balance "
                        f"{money(item.baseline_balance)}."
                    )
                body["target_amount"] = target_amount
                parts.append(f"target {money(item.target_amount)} -> {money(target_amount)}")
            if target_date is not None:
                body["target_date"] = _future_date(target_date).isoformat()
                parts.append(f"date {body['target_date']}")
        preview = f"Update {kind} '{label}': {', '.join(parts)}"
        return stage(state, kind, "PUT", f"{_ROUTES[kind]}/{item_id}", body, preview)

    @tool
    @safe_tool
    def propose_delete(state: State, kind: Kind, item_id: int) -> str:
        """Propose deleting a budget, rule or goal by its id (see the list tools). Not saved yet."""
        _, label = _find(kind, state["user_id"], item_id)
        preview = f"Delete {kind} '{label}'"
        return stage(state, kind, "DELETE", f"{_ROUTES[kind]}/{item_id}", None, preview)

    @tool
    @safe_tool
    def confirm_action(state: State, action_id: str | None = None) -> str:
        """
        Carry out a proposed change, but only after the user has agreed to it.

        Call this only when the user's latest message clearly agrees to the exact change that was
        proposed earlier in the conversation. Never call it in the same turn as a proposal.
        """
        try:
            action = store.take(state["user_id"], state["request_id"], action_id)
        except PendingError as error:
            raise Refusal(str(error)) from None
        payload = action.payload
        if not _route_allowed(payload["method"], payload["path"]):
            return f"NOT saved: {_BAD_ROUTE} Tell the user, and do not claim it was saved."
        try:
            backend.send(action.user_id, payload["method"], payload["path"], payload["body"])
        except BackendError as error:
            return f"NOT saved: {error} Tell the user, and do not claim it was saved."
        return f"Saved: {action.preview}."

    return [
        propose_budget,
        propose_rule,
        propose_goal,
        propose_update,
        propose_delete,
        confirm_action,
    ]
