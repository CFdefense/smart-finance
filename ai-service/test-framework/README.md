# Agent eval framework

Evaluates the `analyze` and `chat` agents with [promptfoo](https://www.promptfoo.dev). A test picks a
persona from `fixtures/sandbox-users.json`, declares its goals, budgets and rules, gives a prompt,
and an LLM judge grades the response for **adherence**, **accuracy** and **goal**.

## Prerequisites

- Node >= 22.22, `cargo`, `sqlx-cli`, `uv`, and Postgres running (`docker compose up -d db`).
- `.env` (repo root) with `DATABASE_URL`, `OPENAI_API_KEY`, `PLAID_CLIENT_ID`, `PLAID_SECRET`,
  `PLAID_ENV=sandbox`. Optional: `AGENT_MODEL`, `EVAL_JUDGE_MODEL` (both default to `gpt-4.1`).
- `cd ai-service && uv sync` and `cd test-framework && npm install`.

## Run

```bash
cd ai-service/test-framework
npm run eval          # every file in ../tests/evals/
npm run view          # browse the results
npm run eval -- --filter-pattern "Best Buy"   # one test
```

Each persona is seeded from the Plaid sandbox once per run into its own throwaway database
(`smartfinance_eval_*`), so a run needs network access and takes a minute or so to start. Your
real database is never modified. Notifications are switched off for the persona, so no email or SMS
is sent. Databases are dropped when the run ends; an interrupted run's leftovers are swept at the
next start.

## Writing a test

Add an entry to a file in `ai-service/tests/evals/`:

```yaml
- description: Best Buy purchase breaks the $200 rule
  vars:
    persona: christian@test.com       # an email in fixtures/sandbox-users.json
    agent: analyze                    # analyze | chat
    goals:                            # optional
      - { name: Trip, description: Save, account: TD Bank Student Checking, target_amount: 2500, days_until_target: 120 }
    budgets:                          # optional, one per category
      - { name: Shopping, category: SHOPPING, monthly_limit: 150 }
    rules:                            # optional; no account = every account
      - { description: Flag any single purchase over $200., account: TD Bank Student Checking }
    transaction:                      # analyze only
      { name: Best Buy, amount: 900, category: SHOPPING, account: TD Bank Student Checking, days_ago: 0 }
    message: Can I afford a TV?       # chat only
    expected: ALERT citing the $200 rule.   # what the goal grader checks
  assert:
    - { type: python, value: "file://graders.py:adherence", config: { threshold: 0.7 } }
    - { type: python, value: "file://graders.py:accuracy", config: { threshold: 0.7 } }
    - { type: python, value: "file://graders.py:goal", config: { threshold: 0.7 } }
```

`account` is an account **name** from the persona's Plaid data. Categories: `SHOPPING`,
`ENTERTAINMENT`, `UTILITIES`, `HEALTHCARE`, `FOOD_DINING`, `GROCERIES`, `TRANSPORTATION`, `OTHER`.
Include only the graders that make sense; `goal` needs `expected`.

| Grader | The judge checks |
|---|---|
| `adherence` | the response respects the seeded rules, budgets and goals |
| `accuracy` | amounts, totals, merchants and dates match the database |
| `goal` | the response achieves what `expected` says |

A bad definition (unknown persona or account, missing field) is reported as an **error**, not a
failed grade.

## Notes

- Always run with `--no-cache` (the npm script does): a cached answer would skip the database setup.
- Plaid's sandbox leaves every transaction category as `OTHER`, so a budget in another category
  shows $0 spent until the persona has spending in that category.
- The code is in this folder as flat modules (`definition`, `evaldb`, `facts`, `runner`, `judge`,
  `graders`, `provider`, `hooks`) and is unit-tested from `ai-service/tests/unit/test_eval_*.py`.
- Evals cost LLM calls and need Plaid credentials, so they are not part of CI.
