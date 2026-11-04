"""
Send a transaction to the running analyzer and print the result.

Start the service in another terminal so you can watch the agent's log lines:

    uv run uvicorn main:app

Then, from ai-service/:

    uv run python tests/manual/trigger.py list                # show the scenarios
    uv run python tests/manual/trigger.py coffee              # run one
    uv run python tests/manual/trigger.py duplicate           # one transaction twice: 2nd is DUPLICATE
    uv run python tests/manual/trigger.py custom --name "Nike" --amount 120 --category SHOPPING

Every run gets a short fresh transaction id (e.g. bestbuy-3f9a) unless you pass --id.
"""

import argparse
import json
import os
import sys
import uuid
from datetime import date

import httpx
import psycopg
from dotenv import find_dotenv, load_dotenv

EMAIL = "christian@test.com"
CHECKING = "TD Bank Student Checking"
CAPITAL_ONE = "Capital One Quicksilver"
DISCOVER = "Discover it Student"

# name -> (merchant, amount, category, account, what to watch for)
SCENARIOS = {
    "coffee": ("Blue Bottle Coffee", 4.50, "FOOD_DINING", CHECKING, "routine: OK, no notification"),
    "bestbuy": ("Best Buy", 900.0, "SHOPPING", CHECKING, "breaks the $200 rule: ALERT, notifies"),
    "dinner": ("Nobu Restaurant", 60.0, "FOOD_DINING", CHECKING, "pushes dining over $100 budget"),
    "casino": ("MGM Grand Casino", 75.0, "ENTERTAINMENT", DISCOVER, "breaks the no-gambling rule"),
    "gas": ("Chevron", 38.0, "TRANSPORTATION", CAPITAL_ONE, "Capital One is dining/travel only"),
    "trip": (
        "Delta Air Lines",
        480.0,
        "TRANSPORTATION",
        CHECKING,
        "big spend against the trip goal",
    ),
}


def lookup(account_name: str) -> tuple[int, str]:
    """Return (user_id, account_id) for Christian's named account."""
    with psycopg.connect(os.environ["DATABASE_URL"]) as conn:
        row = conn.execute(
            """
            SELECT u.id, a.account_id
            FROM users u
            JOIN plaid_links l ON l.user_id = u.id
            JOIN plaid_accounts a ON a.item_id = l.item_id
            WHERE u.email = %s AND a.name = %s
            """,
            (EMAIL, account_name),
        ).fetchone()
    if row is None:
        raise SystemExit("Account not found. Run tests/manual/seed_christian.py first.")
    return row


def analyze(
    url: str,
    user_id: int,
    account_id: str,
    merchant: str,
    amount: float,
    category: str,
    txn_id: str,
) -> tuple[int, dict]:
    """POST one transaction to /analyze and return (http status, json body)."""
    body = {
        "user_id": user_id,
        "transaction": {
            "transaction_id": txn_id,
            "account_id": account_id,
            "name": merchant,
            "amount": amount,
            "date": date.today().isoformat(),
            "pending": False,
            "category": category,
        },
    }
    try:
        response = httpx.post(f"{url}/analyze", json=body, timeout=120)
    except httpx.ConnectError:
        sys.exit(f"Could not reach {url}. Start it with: uv run uvicorn main:app")
    return response.status_code, response.json()


def send_twice(args: argparse.Namespace) -> None:
    """Send the Best Buy purchase twice with one id; the second must come back DUPLICATE."""
    merchant, amount, category, account, _ = SCENARIOS["bestbuy"]
    user_id, account_id = lookup(account)
    txn_id = args.id or f"dup-{uuid.uuid4().hex[:4]}"
    for attempt in ("first", "second"):
        print(f"-> {attempt} send of {merchant} ${amount:.2f} (id={txn_id})")
        status, result = analyze(args.url, user_id, account_id, merchant, amount, category, txn_id)
        verdict = (result.get("assessment") or {}).get("verdict")
        print(
            f"<- HTTP {status} status={result['status']} verdict={verdict} "
            f"notified={result['notified']}"
        )


def main() -> None:
    """Parse arguments, post the transaction and print the analysis."""
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("scenario", choices=[*SCENARIOS, "custom", "duplicate", "list"])
    parser.add_argument("--url", default="http://127.0.0.1:8000")
    parser.add_argument("--id", help="transaction id (default: unique per run)")
    parser.add_argument("--name")
    parser.add_argument("--amount", type=float)
    parser.add_argument("--category", default="OTHER")
    parser.add_argument("--account", default=CHECKING, help="account name for custom")
    args = parser.parse_args()
    load_dotenv(find_dotenv(usecwd=True))

    if args.scenario == "list":
        for key, (merchant, amount, _, account, note) in SCENARIOS.items():
            print(f"{key:8} {merchant} ${amount:.2f} on {account}: {note}")
        return

    if args.scenario == "duplicate":
        send_twice(args)
        return

    if args.scenario == "custom":
        if args.name is None or args.amount is None:
            raise SystemExit("custom needs --name and --amount")
        merchant, amount, category, account = args.name, args.amount, args.category, args.account
    else:
        merchant, amount, category, account, _ = SCENARIOS[args.scenario]

    user_id, account_id = lookup(account)
    txn_id = args.id or f"{args.scenario}-{uuid.uuid4().hex[:4]}"
    print(f"-> {merchant} ${amount:.2f} [{category}] on {account} (id={txn_id})")
    status, result = analyze(args.url, user_id, account_id, merchant, amount, category, txn_id)
    print(f"<- HTTP {status}")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
