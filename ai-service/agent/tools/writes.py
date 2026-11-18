"""Tools: propose changes to budgets, rules and goals, and carry them out once the user agrees."""

from typing import Any

from langchain_core.tools import BaseTool, tool

from agent.backend_api import BackendApi, BackendError
from agent.data_models import Category
from agent.formatting import money
from agent.pending import PendingActionStore, PendingError
from agent.tools import helpers as h
from agent.tools.helpers import Kind, Refusal, State, safe_tool


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
        return h.STAGED.format(ref=action.action_id, preview=preview)

    @tool
    @safe_tool
    def propose_budget(state: State, name: str, category: Category, monthly_limit: float) -> str:
        """
        Propose a new monthly budget for one spending category. It is not saved yet.

        The user must confirm before anything is saved. There is one budget per category.
        """
        clean = h.clean_text(name, "budget name", h.NAME_MAX)
        limit = h.check_limit(monthly_limit)
        existing = h.budget_in(state["user_id"], category)
        if existing is not None:
            raise Refusal(
                f"A budget for {category} already exists (id={existing.id}, "
                f"{money(existing.monthly_limit)}). Use propose_update to change it."
            )
        preview = f"Create budget '{h.label(clean)}' for {category}: {money(limit)} per month"
        body = {"name": clean, "category": category, "monthly_limit": limit}
        return stage(state, "budget", "POST", "/api/budgets", body, preview)

    @tool
    @safe_tool
    def propose_rule(state: State, description: str, account_names: list[str] | None = None) -> str:
        """
        Propose a new rule written in the user's own words. It is not saved yet.

        Leave `account_names` empty for a rule that covers every account. List several accounts
        only when the user named each of them. If a name could mean more than one account, ask
        the user which one instead of listing them all. The user must confirm.
        """
        text = h.clean_text(description, "rule description", h.RULE_MAX)
        accounts = h.accounts_named(state["user_id"], account_names or [])
        preview = f"Create rule '{h.label(text)}' for {h.scope(accounts)}"
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
        clean = h.clean_text(name, "goal name", h.NAME_MAX)
        h.check_target(target_amount)
        uid = state["user_id"]
        depository = h.depository_accounts(uid)
        if not depository:
            raise Refusal(
                "The user has no checking or savings account linked, so no goal can be set."
            )
        account = h.pick_account(depository, account_name, "Goals can use these")
        balance = account.current_balance or 0.0
        if target_amount <= balance:
            raise Refusal(
                f"The target must be more than the current balance {money(balance)} "
                "of that account."
            )
        body: dict[str, Any] = {
            "name": clean,
            "account_id": account.account_id,
            "target_amount": target_amount,
        }
        due = ""
        if target_date is not None:
            body["target_date"] = h.future_date(target_date).isoformat()
            due = f" by {body['target_date']}"
        if description and description.strip():
            body["description"] = h.clean_description(description)
        preview = (
            f"Create goal '{h.label(clean)}' on {h.label(account.name)}: "
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
        extra = sorted(set(given) - h.UPDATABLE[kind])
        if extra:
            raise Refusal(
                f"A {kind} cannot change: {', '.join(extra)}. "
                f"It can change: {', '.join(sorted(h.UPDATABLE[kind]))}."
            )
        item, label = h.find(kind, uid, item_id)
        body: dict[str, Any] = {}
        parts: list[str] = []
        if kind == "budget":
            if name is not None:
                body["name"] = h.clean_text(name, "budget name", h.NAME_MAX)
                parts.append(f"name '{h.label(body['name'])}'")
            if monthly_limit is not None:
                body["monthly_limit"] = h.check_limit(monthly_limit)
                parts.append(f"limit {money(item.monthly_limit)} -> {money(monthly_limit)}")
        elif kind == "rule":
            if description is None:
                text = item.description
            else:
                text = h.clean_text(description, "rule description", h.RULE_MAX)
            ids = item.account_ids
            if account_names is not None:
                accounts = h.accounts_named(uid, account_names)
                ids = [a.account_id for a in accounts]
                parts.append(f"applies to {h.scope(accounts)}")
            if description is not None:
                parts.insert(0, f"text '{h.label(text)}'")
            body = {"description": text, "account_ids": ids, "enabled": True}
        else:
            if name is not None:
                body["name"] = h.clean_text(name, "goal name", h.NAME_MAX)
                parts.append(f"name '{h.label(body['name'])}'")
            if description is not None:
                body["description"] = h.clean_description(description)
                parts.append("description")
            if target_amount is not None:
                h.check_target(target_amount)
                if target_amount <= item.baseline_balance:
                    raise Refusal(
                        "The target must be more than the starting balance "
                        f"{money(item.baseline_balance)}."
                    )
                body["target_amount"] = target_amount
                parts.append(f"target {money(item.target_amount)} -> {money(target_amount)}")
            if target_date is not None:
                body["target_date"] = h.future_date(target_date).isoformat()
                parts.append(f"date {body['target_date']}")
        preview = f"Update {kind} '{label}': {', '.join(parts)}"
        return stage(state, kind, "PUT", f"{h.ROUTES[kind]}/{item_id}", body, preview)

    @tool
    @safe_tool
    def propose_delete(state: State, kind: Kind, item_id: int) -> str:
        """Propose deleting a budget, rule or goal by its id (see the list tools). Not saved yet."""
        _, label = h.find(kind, state["user_id"], item_id)
        preview = f"Delete {kind} '{label}'"
        return stage(state, kind, "DELETE", f"{h.ROUTES[kind]}/{item_id}", None, preview)

    @tool
    @safe_tool
    def confirm_action(state: State, action_id: str | None = None) -> str:
        """
        Carry out a proposed change, but only after the user has agreed to it.

        Call this only when the user's latest message clearly agrees to the exact change that was
        proposed earlier in the conversation. Never call it in the same turn as a proposal.
        Leave `action_id` empty unless a tool result listed several waiting changes. Never invent
        a reference.
        """
        try:
            action = store.take(state["user_id"], state["request_id"], action_id)
        except PendingError as error:
            raise Refusal(str(error)) from None
        payload = action.payload
        if not h.route_allowed(payload["method"], payload["path"]):
            return f"NOT saved: {h.BAD_ROUTE} Tell the user, and do not claim it was saved."
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
