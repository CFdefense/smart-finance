"""Snapshot of a persona's data, given to the graders as ground truth."""

from datetime import date, timedelta

from agent import MAX_TRANSACTIONS, RECENT_DAYS
from db import load_accounts, load_budgets, load_goals, load_rules, load_transactions


def snapshot(user_id: int, today: date | None = None) -> dict:
    """Return the user's accounts, recent transactions, budgets, rules and goals as plain data."""
    today = today or date.today()
    accounts = load_accounts(user_id)
    names = {a.account_id: a.name for a in accounts}

    rules = {}
    for account in accounts:
        for rule in load_rules(user_id, account.account_id):
            rules[rule.id] = {
                "description": rule.description,
                "applies_to": [names.get(i, i) for i in rule.account_ids] or "all accounts",
            }

    transactions = load_transactions(
        user_id, since=today - timedelta(days=RECENT_DAYS), limit=MAX_TRANSACTIONS
    )
    return {
        "today": today.isoformat(),
        "accounts": [a.model_dump(mode="json") for a in accounts],
        "transactions": [t.model_dump(mode="json") for t in transactions],
        "budgets": [b.model_dump(mode="json") for b in load_budgets(user_id, today)],
        "rules": list(rules.values()),
        "goals": [g.model_dump(mode="json") for g in load_goals(user_id)],
    }
