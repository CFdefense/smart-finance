"""
Give the sandbox user christian@test.com budgets, rules, goals and recent spending.

Run `cargo run --bin seed-sandbox` first (it creates the user and accounts), then, from
ai-service/:

    uv run python tests/manual/seed_christian.py

Safe to re-run: it replaces Christian's budgets, rules, goals, demo transactions and past analyses.
"""

import os
from datetime import date, timedelta

import psycopg
from dotenv import find_dotenv, load_dotenv

EMAIL = "christian@test.com"
DEMO_PREFIX = "demo_"

# Plaid's sandbox leaves everything as OTHER, so categorize the transactions the seeder created.
RECATEGORIZE = {
    "Starbucks": "FOOD_DINING",
    "DOORDASH*CHIPOTLE": "FOOD_DINING",
    "Uber": "TRANSPORTATION",
    "AMAZON MKTPL*7R3LQ2": "SHOPPING",
}

# (budget name, category, monthly limit)
BUDGETS = [
    ("Eating out", "FOOD_DINING", 100.0),
    ("Shopping", "SHOPPING", 150.0),
    ("Fun money", "ENTERTAINMENT", 40.0),
    ("Groceries", "GROCERIES", 300.0),
]

# (description, account name the rule applies to, or None for every account)
RULES = [
    ("Flag any single purchase over $200.", "TD Bank Student Checking"),
    ("No gambling, casinos or betting sites on any account.", None),
    ("The Capital One card is for dining and travel only.", "Capital One Quicksilver"),
]

# (name, description, account name, target amount, days until target date)
GOALS = [
    ("Spring break trip", "Save for a trip with friends.", "TD Bank Student Checking", 2500, 120),
    ("Roth IRA contribution", "Add $500 to the Roth IRA.", "Robinhood Roth IRA", 1000, 60),
]

# Spending already on the books this month (days ago, name, amount, category, account name).
# Dining adds up to $82 of the $100 budget, so a single dinner tips it over.
THIS_MONTH = [
    (3, "Chipotle", 14.50, "FOOD_DINING", "TD Bank Student Checking"),
    (3, "Trader Joe's", 62.30, "GROCERIES", "TD Bank Student Checking"),
    (2, "Starbucks", 6.75, "FOOD_DINING", "TD Bank Student Checking"),
    (2, "Shell", 41.00, "TRANSPORTATION", "Capital One Quicksilver"),
    (1, "Domino's Pizza", 27.80, "FOOD_DINING", "Discover it Student"),
    (1, "Target", 98.40, "SHOPPING", "Discover it Student"),
    (0, "Panera Bread", 33.00, "FOOD_DINING", "TD Bank Student Checking"),
]


def main() -> None:
    """Replace Christian's demo data."""
    load_dotenv(find_dotenv(usecwd=True))
    today = date.today()
    with psycopg.connect(os.environ["DATABASE_URL"]) as conn:
        row = conn.execute("SELECT id FROM users WHERE email = %s", (EMAIL,)).fetchone()
        if row is None:
            raise SystemExit(f"{EMAIL} not found. Run `cargo run --bin seed-sandbox` first.")
        user_id = row[0]

        accounts = dict(
            conn.execute(
                """
                SELECT a.name, a.account_id
                FROM plaid_accounts a JOIN plaid_links l ON l.item_id = a.item_id
                WHERE l.user_id = %s
                """,
                (user_id,),
            ).fetchall()
        )

        # Clear previous demo state.
        for table in ("budgets", "rules", "goals", "transaction_analyses"):
            conn.execute(f"DELETE FROM {table} WHERE user_id = %s", (user_id,))
        conn.execute(
            "DELETE FROM plaid_transactions WHERE transaction_id LIKE %s", (DEMO_PREFIX + "%",)
        )

        for name, category in RECATEGORIZE.items():
            conn.execute(
                "UPDATE plaid_transactions SET category = %s WHERE name = %s", (category, name)
            )

        for name, category, limit in BUDGETS:
            conn.execute(
                "INSERT INTO budgets (user_id, name, category, monthly_limit) "
                "VALUES (%s, %s, %s, %s)",
                (user_id, name, category, limit),
            )

        for description, account_name in RULES:
            rule_id = conn.execute(
                "INSERT INTO rules (user_id, description) VALUES (%s, %s) RETURNING id",
                (user_id, description),
            ).fetchone()[0]
            if account_name:
                conn.execute(
                    "INSERT INTO rule_accounts (rule_id, account_id) VALUES (%s, %s)",
                    (rule_id, accounts[account_name]),
                )

        for name, description, account_name, target, days in GOALS:
            baseline = conn.execute(
                "SELECT current_balance FROM plaid_accounts WHERE account_id = %s",
                (accounts[account_name],),
            ).fetchone()[0]
            conn.execute(
                "INSERT INTO goals (user_id, account_id, name, description, target_amount, "
                "baseline_balance, target_date) VALUES (%s, %s, %s, %s, %s, %s, %s)",
                (
                    user_id,
                    accounts[account_name],
                    name,
                    description,
                    target,
                    baseline,
                    today + timedelta(days=days),
                ),
            )

        month_start = today.replace(day=1)
        for i, (days_ago, name, amount, category, account_name) in enumerate(THIS_MONTH):
            when = max(today - timedelta(days=days_ago), month_start)
            conn.execute(
                "INSERT INTO plaid_transactions (transaction_id, account_id, name, amount, date, "
                "pending, category) VALUES (%s, %s, %s, %s, %s, false, %s)",
                (f"{DEMO_PREFIX}{i}", accounts[account_name], name, amount, when, category),
            )

    print(f"Seeded user {user_id} ({EMAIL}):")
    print(f"  {len(BUDGETS)} budgets, {len(RULES)} rules, {len(GOALS)} goals")
    print(f"  {len(THIS_MONTH)} transactions this month")
    print("Accounts:")
    for name, account_id in accounts.items():
        print(f"  {name}: {account_id}")


if __name__ == "__main__":
    main()
