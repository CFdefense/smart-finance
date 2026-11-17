"""Integration tests: these need Postgres running with the migrations applied."""

import uuid
from datetime import date

import pytest

from db import (
    category_totals,
    get_connection,
    load_accounts,
    load_all_rules,
    load_budgets,
    load_goals,
    load_preferences,
    load_transactions,
    merchant_totals,
    net_flow,
)

# (name, amount, category, date)
TRANSACTIONS = [
    ("coffee", 5, "FOOD_DINING", "2026-10-01"),
    ("lunch", 15, "FOOD_DINING", "2026-10-03"),
    ("refund", -20, "FOOD_DINING", "2026-10-04"),
    ("groceries", 60, "GROCERIES", "2026-10-02"),
    ("tv", 900, "SHOPPING", "2026-09-15"),
]


@pytest.fixture
def user_with_data():
    """Create a user with one account, transactions, a budget and a goal; delete them after."""
    tag = uuid.uuid4().hex[:8]  # unique per test run, so runs never collide
    with get_connection() as conn:
        user_id = conn.execute(
            "INSERT INTO users (email, password_hash) VALUES (%s, 'x') RETURNING id",
            (f"test-{tag}@example.com",),
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES (%s, %s, 'tok')",
            (user_id, f"item-{tag}"),
        )
        conn.execute(
            "INSERT INTO plaid_accounts (account_id, item_id, name, account_type, current_balance) "
            "VALUES (%s, %s, 'Checking', 'depository', 1500)",
            (f"acc-{tag}", f"item-{tag}"),
        )
        for name, amount, category, on in TRANSACTIONS:
            conn.execute(
                "INSERT INTO plaid_transactions "
                "(transaction_id, account_id, name, amount, date, category) "
                "VALUES (%s, %s, %s, %s, %s, %s)",
                (f"{name}-{tag}", f"acc-{tag}", name, amount, on, category),
            )
        conn.execute(
            "INSERT INTO budgets (user_id, name, category, monthly_limit) "
            "VALUES (%s, 'Eating out', 'FOOD_DINING', 100)",
            (user_id,),
        )
        conn.execute(
            "INSERT INTO goals (user_id, account_id, name, target_amount, baseline_balance) "
            "VALUES (%s, %s, 'Emergency fund', 5000, 1000)",
            (user_id, f"acc-{tag}"),
        )

    yield user_id, tag

    # Deleting the user also deletes everything linked to them (ON DELETE CASCADE).
    with get_connection() as conn:
        conn.execute("DELETE FROM users WHERE id = %s", (user_id,))


def test_load_transactions_returns_the_users_transactions(user_with_data):
    """All of the user's transactions come back, newest first, with their categories."""
    user_id, tag = user_with_data

    transactions = load_transactions(user_id)

    assert [t.transaction_id for t in transactions] == [
        f"{name}-{tag}" for name in ["refund", "lunch", "groceries", "coffee", "tv"]
    ]
    tv = transactions[-1]
    assert tv.amount == 900
    assert tv.category == "SHOPPING"


def test_load_transactions_since_and_limit(user_with_data):
    """`since` drops older transactions and `limit` keeps only the newest."""
    user_id, _ = user_with_data

    assert "tv" not in [t.name for t in load_transactions(user_id, since=date(2026, 10, 1))]
    assert [t.name for t in load_transactions(user_id, limit=2)] == ["refund", "lunch"]


def test_load_transactions_never_returns_other_users_transactions(user_with_data):
    """A different user ID gets none of this user's transactions."""
    user_id, _ = user_with_data

    assert load_transactions(user_id + 999_999) == []


def test_load_accounts(user_with_data):
    """The user's account comes back with its balance."""
    user_id, tag = user_with_data

    [account] = load_accounts(user_id)

    assert account.account_id == f"acc-{tag}"
    assert account.current_balance == 1500


def test_load_budgets_counts_only_money_out_in_that_month(user_with_data):
    """Spending sums positive amounts in the budget's category for the month only."""
    user_id, _ = user_with_data

    [october] = load_budgets(user_id, today=date(2026, 10, 20))
    [september] = load_budgets(user_id, today=date(2026, 9, 20))

    assert october.spent_this_month == 20  # coffee + lunch; the refund is ignored
    assert september.spent_this_month == 0


def test_load_goals_includes_account_balance(user_with_data):
    """Goals come back with the linked account's name and current balance."""
    user_id, _ = user_with_data

    [goal] = load_goals(user_id)

    assert goal.account_name == "Checking"
    assert goal.current_balance == 1500
    assert goal.target_amount == 5000


def test_load_preferences_raises_for_unknown_user(user_with_data):
    """A user ID that doesn't exist raises instead of returning empty preferences."""
    user_id, _ = user_with_data

    assert load_preferences(user_id).email.startswith("test-")
    with pytest.raises(ValueError):
        load_preferences(user_id + 999_999)


def test_load_budgets_and_goals_include_ids(user_with_data):
    """Budgets and goals carry their ids, and goals their account id."""
    user_id, tag = user_with_data

    [budget] = load_budgets(user_id, today=date(2026, 10, 20))
    [goal] = load_goals(user_id)

    assert isinstance(budget.id, int)
    assert isinstance(goal.id, int) and goal.account_id == f"acc-{tag}"


def test_load_all_rules_returns_enabled_rules_for_any_account(user_with_data):
    """Enabled rules come back with their account ids; disabled and other users' rules do not."""
    user_id, tag = user_with_data
    with get_connection() as conn:
        rule_id = conn.execute(
            "INSERT INTO rules (user_id, description) VALUES (%s, 'No gambling') RETURNING id",
            (user_id,),
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO rule_accounts (rule_id, account_id) VALUES (%s, %s)",
            (rule_id, f"acc-{tag}"),
        )
        conn.execute(
            "INSERT INTO rules (user_id, description, enabled) VALUES (%s, 'Old', false)",
            (user_id,),
        )

    rules = load_all_rules(user_id)

    assert [(r.description, r.account_ids) for r in rules] == [("No gambling", [f"acc-{tag}"])]
    assert load_all_rules(user_id + 999_999) == []


def test_category_totals_sum_only_money_out_in_the_window(user_with_data):
    """Positive amounts per category inside [start, end); the refund and the September TV are out."""
    user_id, _ = user_with_data

    totals = category_totals(user_id, date(2026, 10, 1), date(2026, 11, 1))

    assert totals == {"GROCERIES": 60, "FOOD_DINING": 20}
    assert list(totals) == ["GROCERIES", "FOOD_DINING"]  # biggest first


def test_category_totals_end_is_exclusive(user_with_data):
    """A transaction on the end date is not counted."""
    user_id, _ = user_with_data

    assert category_totals(user_id, date(2026, 10, 1), date(2026, 10, 2)) == {"FOOD_DINING": 5}


def test_merchant_totals_group_and_limit(user_with_data):
    """Merchants are ranked by total spend and limited."""
    user_id, _ = user_with_data

    top = merchant_totals(user_id, date(2026, 9, 1), date(2026, 11, 1), limit=2)

    assert [(m.name, m.total, m.count) for m in top] == [("tv", 900, 1), ("groceries", 60, 1)]


def test_net_flow_is_deposits_minus_withdrawals_on_depository_accounts(user_with_data):
    """Money in minus money out; the refund counts as money in."""
    user_id, _ = user_with_data

    assert net_flow(user_id, date(2026, 10, 1), date(2026, 11, 1)) == -60


def test_analytics_never_include_other_users(user_with_data):
    """Another user id sees nothing."""
    user_id, _ = user_with_data
    other = user_id + 999_999

    assert category_totals(other, date(2026, 1, 1), date(2027, 1, 1)) == {}
    assert merchant_totals(other, date(2026, 1, 1), date(2027, 1, 1), 5) == []
    assert net_flow(other, date(2026, 1, 1), date(2027, 1, 1)) == 0


def test_load_transactions_filters_by_category(user_with_data):
    """Only the requested category comes back, and the limit applies after the filter."""
    user_id, _ = user_with_data

    assert [t.name for t in load_transactions(user_id, category="FOOD_DINING")] == [
        "refund",
        "lunch",
        "coffee",
    ]
    assert [t.name for t in load_transactions(user_id, category="FOOD_DINING", limit=1)] == [
        "refund"
    ]
    assert load_transactions(user_id, category="TRAVEL") == []


def test_load_transactions_filters_by_merchant_ignoring_case(user_with_data):
    """The merchant filter is a case-insensitive 'contains' match."""
    user_id, _ = user_with_data

    assert [t.name for t in load_transactions(user_id, merchant="GROCER")] == ["groceries"]
    assert [t.name for t in load_transactions(user_id, merchant="LUN", category="FOOD_DINING")] == [
        "lunch"
    ]


def test_load_transactions_merchant_wildcards_match_literally(user_with_data):
    """`%` and `_` in the merchant text are not wildcards."""
    user_id, _ = user_with_data

    assert load_transactions(user_id, merchant="%") == []
    assert load_transactions(user_id, merchant="c_ffee") == []
    assert load_transactions(user_id, merchant="a\\b") == []
    assert [t.name for t in load_transactions(user_id, merchant="coffee")] == ["coffee"]
