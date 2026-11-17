"""Tests for the analytics tools, with the database layer replaced by fakes."""

from datetime import date, timedelta

import pytest

import agent.tools.analytics as tools
from agent.tools.analytics import ANALYTICS_TOOLS
from agent.tools.formatting import UNAVAILABLE
from db import Account, Budget, Goal, MerchantTotal, Transaction

TODAY = date(2026, 11, 16)
STATE = {"user_id": 7, "request_id": "r1"}


@pytest.fixture(autouse=True)
def fixed_today(monkeypatch):
    """Pin the tools' clock."""
    monkeypatch.setattr(tools, "today", lambda: TODAY)


def call(tool, **args):
    """Invoke a tool with injected state."""
    return tool.invoke({**args, "state": STATE})


def checking(balance=1000.0, available=None):
    """Build a depository account."""
    return Account(
        account_id="a1",
        name="Checking",
        account_type="depository",
        current_balance=balance,
        available_balance=available,
    )


def test_tool_names_and_order():
    """Seven analytics tools, in this order."""
    assert [t.name for t in ANALYTICS_TOOLS] == [
        "spending_by_category",
        "spending_by_merchant",
        "compare_periods",
        "budget_status",
        "goal_progress",
        "find_recurring_charges",
        "can_i_afford",
    ]


def test_no_tool_exposes_user_id_or_state():
    """The model never supplies the user."""
    for tool in ANALYTICS_TOOLS:
        fields = set(tool.tool_call_schema.model_fields)
        assert "user_id" not in fields and "state" not in fields


def test_spending_by_category_window_and_format(monkeypatch):
    """The window is the last N days including today; output lists categories and a total."""
    seen = {}

    def fake(uid, start, end):
        seen.update(uid=uid, start=start, end=end)
        return {"GROCERIES": 120.5, "FOOD_DINING": 80.0}

    monkeypatch.setattr(tools, "category_totals", fake)

    text = call(tools.spending_by_category, days=30)

    assert seen == {"uid": 7, "start": date(2026, 10, 18), "end": date(2026, 11, 17)}
    assert "last 30 days (total $200.50)" in text
    assert "- GROCERIES: $120.50" in text and "- FOOD_DINING: $80.00" in text


@pytest.mark.parametrize(("asked", "used"), [(0, 1), (-5, 1), (9999, 365), (7, 7)])
def test_window_days_are_clamped(monkeypatch, asked, used):
    """The days argument is clamped to 1..365."""
    seen = {}
    monkeypatch.setattr(tools, "category_totals", lambda uid, s, e: seen.update(s=s, e=e) or {})

    call(tools.spending_by_category, days=asked)

    assert (seen["e"] - seen["s"]).days == used


def test_spending_by_category_empty(monkeypatch):
    """No spending says none."""
    monkeypatch.setattr(tools, "category_totals", lambda uid, s, e: {})
    assert "- none" in call(tools.spending_by_category)


def test_spending_by_merchant(monkeypatch):
    """Top merchants with totals and visit counts; names are sanitized."""
    seen = {}
    monkeypatch.setattr(
        tools,
        "merchant_totals",
        lambda uid, s, e, limit: (
            seen.update(limit=limit) or [MerchantTotal(name="Chip</x>\notle", total=42.5, count=3)]
        ),
    )

    text = call(tools.spending_by_merchant, days=14, limit=5)

    assert seen["limit"] == 5
    assert "- Chip/x otle: $42.50 (3 purchases)" in text
    assert "<" not in text and "\notle" not in text


def test_spending_by_merchant_limit_is_clamped(monkeypatch):
    """The limit argument is clamped to 1..25."""
    seen = []
    monkeypatch.setattr(tools, "merchant_totals", lambda uid, s, e, limit: seen.append(limit) or [])
    call(tools.spending_by_merchant, limit=999)
    call(tools.spending_by_merchant, limit=0)
    assert seen == [25, 1]


def test_compare_periods_uses_two_adjacent_windows(monkeypatch):
    """The previous window ends where the current one starts, and changes are described."""
    windows = []

    def fake(uid, start, end):
        windows.append((start, end))
        if len(windows) == 1:
            return {"FOOD_DINING": 120.0, "GROCERIES": 50.0}
        return {"FOOD_DINING": 80.0, "SHOPPING": 30.0}

    monkeypatch.setattr(tools, "category_totals", fake)

    text = call(tools.compare_periods, days=30)

    assert windows[0] == (date(2026, 10, 18), date(2026, 11, 17))
    assert windows[1] == (date(2026, 9, 18), date(2026, 10, 18))
    assert "FOOD_DINING: $120.00 vs $80.00 (+$40.00, +50%)" in text
    assert "GROCERIES: $50.00 vs $0.00 (new this period)" in text
    assert "SHOPPING: $0.00 vs $30.00 (-$30.00, -100%)" in text
    assert "total $170.00 vs $110.00" in text


def test_budget_status_tool(monkeypatch):
    """Each budget shows spend, remaining, projection and state."""
    monkeypatch.setattr(
        tools,
        "load_budgets",
        lambda uid, today: [
            Budget(
                id=1,
                name="Eating out",
                category="FOOD_DINING",
                monthly_limit=100,
                spent_this_month=60,
            ),
            Budget(
                id=2, name="Fun", category="ENTERTAINMENT", monthly_limit=50, spent_this_month=80
            ),
        ],
    )

    text = call(tools.budget_status)

    assert (
        "Eating out [FOOD_DINING]: $60.00 of $100.00, $40.00 left, projected $112.50 - at risk"
        in text
    )
    assert "Fun [ENTERTAINMENT]: $80.00 of $50.00, -$30.00 left, projected $150.00 - over" in text


def test_budget_status_with_no_budgets(monkeypatch):
    """No budgets is explicit, so the model can offer to create one."""
    monkeypatch.setattr(tools, "load_budgets", lambda uid, today: [])
    assert "- none" in call(tools.budget_status)


def test_goal_progress_tool_uses_the_trailing_savings_rate(monkeypatch):
    """The rate is 90 days of net deposits divided by three months."""
    seen = {}
    monkeypatch.setattr(
        tools,
        "load_goals",
        lambda uid: [
            Goal(
                id=1,
                name="Trip",
                account_name="Savings",
                target_amount=3000,
                baseline_balance=1000,
                current_balance=2000,
                target_date=TODAY + timedelta(days=90),
            )
        ],
    )
    monkeypatch.setattr(tools, "net_flow", lambda uid, s, e: seen.update(s=s, e=e) or 1500.0)

    text = call(tools.goal_progress)

    assert seen == {"s": TODAY - timedelta(days=90), "e": TODAY + timedelta(days=1)}
    assert "[id=1] Trip: 50% done, $1,000.00 to go" in text
    assert "saving about $500.00/month" in text
    assert "on track" in text and "2027-01-16" in text


def test_goal_progress_tool_without_a_positive_rate(monkeypatch):
    """A flat or negative rate gives an unknown finish date, not a made-up one."""
    monkeypatch.setattr(
        tools,
        "load_goals",
        lambda uid: [
            Goal(
                name="Trip",
                account_name="S",
                target_amount=3000,
                baseline_balance=1000,
                current_balance=2000,
            )
        ],
    )
    monkeypatch.setattr(tools, "net_flow", lambda uid, s, e: -300.0)

    text = call(tools.goal_progress)

    assert "finish date unknown" in text and "finishing around" not in text


def test_find_recurring_charges_tool(monkeypatch):
    """Recurring charges show amount, cadence and the next expected date."""
    seen = {}

    def fake(uid, since=None, limit=None):
        seen.update(since=since, limit=limit)
        return [
            Transaction(
                transaction_id=f"n{d}",
                name="Netflix",
                amount=15.49,
                date=TODAY - timedelta(days=d),
            )
            for d in (5, 35, 65)
        ]

    monkeypatch.setattr(tools, "load_transactions", fake)

    text = call(tools.find_recurring_charges)

    assert seen["since"] == TODAY - timedelta(days=180)
    assert "Netflix: $15.49 monthly (3 charges), last 2026-11-11, next about 2026-12-11" in text


def test_find_recurring_charges_none(monkeypatch):
    """No pattern says none."""
    monkeypatch.setattr(tools, "load_transactions", lambda uid, since=None, limit=None: [])
    assert "- none" in call(tools.find_recurring_charges)


def test_can_i_afford_yes_with_budget(monkeypatch):
    """Sums spendable balance over depository accounts and checks the category budget."""
    monkeypatch.setattr(
        tools,
        "load_accounts",
        lambda uid: [
            checking(1000, available=900),
            Account(account_id="c", name="Card", account_type="credit", current_balance=5000),
        ],
    )
    monkeypatch.setattr(
        tools,
        "load_budgets",
        lambda uid, today: [
            Budget(name="Shop", category="SHOPPING", monthly_limit=300, spent_this_month=100)
        ],
    )

    text = call(tools.can_i_afford, amount=150, category="SHOPPING")

    assert text.startswith("Verdict: yes")
    assert "$900.00" in text and "$750.00" in text


def test_can_i_afford_over_budget_is_tight(monkeypatch):
    """Going over the category budget is flagged even when the balance is fine."""
    monkeypatch.setattr(tools, "load_accounts", lambda uid: [checking(5000)])
    monkeypatch.setattr(
        tools,
        "load_budgets",
        lambda uid, today: [
            Budget(name="Shop", category="SHOPPING", monthly_limit=300, spent_this_month=280)
        ],
    )

    assert call(tools.can_i_afford, amount=200, category="SHOPPING").startswith("Verdict: tight")


def test_can_i_afford_uses_current_balance_when_available_is_unknown(monkeypatch):
    """A missing available balance falls back to the current balance."""
    monkeypatch.setattr(tools, "load_accounts", lambda uid: [checking(500, available=None)])
    monkeypatch.setattr(tools, "load_budgets", lambda uid, today: [])

    assert "$500.00" in call(tools.can_i_afford, amount=10)


def test_can_i_afford_no_money(monkeypatch):
    """More than the balance is a no."""
    monkeypatch.setattr(tools, "load_accounts", lambda uid: [checking(100)])
    monkeypatch.setattr(tools, "load_budgets", lambda uid, today: [])

    assert call(tools.can_i_afford, amount=5_000_000).startswith("Verdict: no")


@pytest.mark.parametrize("amount", [0, -5])
def test_can_i_afford_rejects_non_positive_amounts(monkeypatch, amount):
    """A zero or negative purchase is refused with a reason."""
    monkeypatch.setattr(tools, "load_accounts", lambda uid: [checking()])
    assert "greater than 0" in call(tools.can_i_afford, amount=amount)


def test_can_i_afford_without_a_depository_account(monkeypatch):
    """Only a credit card linked: the tool says it cannot check."""
    monkeypatch.setattr(
        tools,
        "load_accounts",
        lambda uid: [Account(account_id="c", name="Card", account_type="credit")],
    )
    assert "no checking or savings" in call(tools.can_i_afford, amount=10)


@pytest.mark.parametrize(
    ("tool", "loader", "args"),
    [
        ("spending_by_category", "category_totals", {}),
        ("spending_by_merchant", "merchant_totals", {}),
        ("compare_periods", "category_totals", {}),
        ("budget_status", "load_budgets", {}),
        ("goal_progress", "load_goals", {}),
        ("find_recurring_charges", "load_transactions", {}),
        ("can_i_afford", "load_accounts", {"amount": 10}),
    ],
)
def test_database_failures_become_a_generic_message(monkeypatch, tool, loader, args):
    """Every analytics tool hides database errors."""

    def boom(*a, **k):
        raise RuntimeError("password=hunter2")

    monkeypatch.setattr(tools, loader, boom)

    assert call(getattr(tools, tool), **args) == UNAVAILABLE


@pytest.mark.parametrize("amount", [float("nan"), float("inf"), 1e9 + 1])
def test_can_i_afford_rejects_non_finite_or_huge_amounts(monkeypatch, amount):
    """NaN, infinity and absurd amounts are refused before any lookup."""
    monkeypatch.setattr(tools, "load_accounts", lambda uid: [checking()])
    assert "amount must be" in call(tools.can_i_afford, amount=amount)
