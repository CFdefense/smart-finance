# Tests

| Folder | What it is | How to run |
|---|---|---|
| `unit/` | Fast pytest tests. No network, database or LLM. Includes the tests for the eval framework itself (`test_eval_*.py`). | `uv run pytest` |
| `integration/` | pytest tests for the HTTP routes and the database queries. The query tests need Postgres with the migrations applied. | `uv run pytest` |
| `evals/` | **Agent evals**: YAML test definitions that run the real `analyze` and `chat` agents and have an LLM grade the answer. | `npm run eval` (see below) |
| `*.py`, `promptfooconfig.yaml`, `package.json` (top level of `tests/`) | The promptfoo-based framework that runs the evals. | |

## Agent evals

An eval picks a persona from `fixtures/sandbox-users.json`, sets that persona's goals, budgets and
rules, gives a prompt, and runs one of the agents:

- `analyze` takes a **transaction** as its prompt.
- `chat` takes a chat **message** as its prompt.

Three LLM graders then judge the response: **adherence**, **accuracy** and **goal**.

### Prerequisites

Ensure you have Node >= 22.22, `cargo`, `sqlx-cli`, `uv`, and Docker installed.

1. **Environment variables:** Ensure your `.env` (in the repo root) has:
   - `DATABASE_URL` (plain `postgres://user:pw@host:port/db` with permissions to create and drop databases)
   - `OPENAI_API_KEY`
   - `PLAID_CLIENT_ID`, `PLAID_SECRET`, `PLAID_ENV=sandbox`
   - *Optional:* `AGENT_MODEL` (agent under test) and `EVAL_JUDGE_MODEL` (grader LLM); both default to `gpt-4.1`.

2. **Setup and Install:**

```bash
# 1. Start Postgres (from repo root)
docker compose up -d db

# 2. Set up Python environment & dependencies
cd ai-service
uv sync

# 3. Install promptfoo locally
cd tests
npm install
```

### Run the evals

```bash
cd ai-service/tests

# Run every test in evals/
npm run eval

# Run a specific test matching a description
npm run eval -- --filter-pattern "Best Buy"

# Launch the web UI to inspect scores & judge reasoning
npm run view
```

`promptfoo eval` exits non-zero when any test fails. Do not run two evals at once: the start of each
run sweeps every `smartfinance_eval_*` database.

Evals cost LLM calls and need Plaid credentials, so they are **not** part of CI.

### Write an eval

Add an entry to a file in `evals/` (or create a new `*.yaml` file there; every file in the
folder is picked up). Name files `NN_<analyze|chat>_<testname>.yaml` (for example
`02_analyze_bestbuy.yaml`), one test per file. Each entry is one test:

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

#### Fields (`vars`)

| Field | Required | Meaning |
|---|---|---|
| `persona` | yes | Email of a user in `fixtures/sandbox-users.json`. That one user is seeded from the Plaid sandbox into the test database. |
| `agent` | yes | `analyze` or `chat`. |
| `transaction` | `analyze` | The transaction to analyze: `name`, `amount`, `account` (required); `category` (default `OTHER`) and `days_ago` (default 0) optional. It is passed to the agent as a new transaction and is **not** stored in the database. |
| `message` | `chat` | The chat message sent to the agent. |
| `goals` | no | Savings goals: `name`, `account`, `target_amount` (required); `description`, `days_until_target` optional. |
| `budgets` | no | Monthly budgets: `name`, `category`, `monthly_limit` (all required). One per category. |
| `rules` | no | Natural-language rules: `description` (required); `account` optional (no account = every account). |
| `expected` | for `goal` | What a good response achieves, in plain words. Only the `goal` grader reads it. |

- `account` is an account **name** from the persona's Plaid data (for `christian@test.com`: for
  example `TD Bank Student Checking`). It is resolved to the Plaid account id for you.
- Categories: `SHOPPING`, `ENTERTAINMENT`, `UTILITIES`, `HEALTHCARE`, `FOOD_DINING`, `GROCERIES`,
  `TRANSPORTATION`, `OTHER`.
- Before each test the persona's goals, budgets, rules and past analyses are cleared and replaced
  with the ones in the test, so tests do not affect each other.

#### Graders (`assert`)

Include any of these; each is optional. `config.threshold` (0 to 1, default 0.7) is the lowest score
that passes.

| Grader | The judge checks |
|---|---|
| `file://graders.py:adherence` | the response respects the seeded rules, budgets and goals |
| `file://graders.py:accuracy` | amounts, totals, merchants and dates match the database |
| `file://graders.py:goal` | the response achieves what `expected` says |

The judge sees the request, the agent's response, and a snapshot of the persona's data (accounts,
recent transactions, budgets with spend, rules, goals). Use `npm run view` to read each grader's
score and reason.

A bad definition (unknown persona or account, missing field) is reported as an **error**, not a
failed grade, with a message saying what to fix.

### What to know when writing evals

- The Plaid sandbox puts every transaction in category `OTHER`, so only a budget on `OTHER` ever
  shows real spend. Budgets in other categories always show $0 spent.
- The chat agent is not given the user's rules (only accounts, budgets, goals and recent
  transactions), so chat tests should not expect rule recall. The analyze agent does receive rules.
- Notifications are switched off for the persona, so no email or SMS is sent. Results still report
  `notified: true` for an ALERT.
- Keep `expected` specific and checkable ("Verdict ALERT and cites the $200 rule"), not vague
  ("a good answer").

### How it works

For each test the provider (`provider.py`):

1. Creates a throwaway database `smartfinance_eval_*` for the persona the first time it is used in a
   run (migrations, then the existing `seed-sandbox` binary against the Plaid sandbox, for only that
   persona). Your real database is never modified.
2. Resets and inserts the test's goals, budgets and rules.
3. Runs the `analyze` or `chat` agent in-process against that database.
4. Returns the response plus a snapshot of the persona's data to the graders.

The databases are dropped when the run ends; leftovers from an interrupted run are swept at the next
start. The first run compiles `seed-sandbox` (`cargo build`), which can take a few minutes on a cold
build, and each persona's first seed calls Plaid. The provider is pinned to one worker with a 15
minute timeout so that cold run can finish.

### Framework code

Flat modules at the top level of `tests/`, imported as top-level modules:

| Module | Responsibility |
|---|---|
| `definition.py` | parse and validate a test's `vars` |
| `evaldb.py` | create, seed, reset and drop the per-persona databases |
| `runner.py` | run the `analyze` or `chat` agent in-process |
| `facts.py` | snapshot of the persona's data for the graders |
| `judge.py`, `graders.py` | the judge LLM and the three graders |
| `provider.py`, `hooks.py` | the promptfoo provider and cleanup hook |
| `promptfooconfig.yaml` | promptfoo wiring (provider, hook, `evals/*.yaml`, one test at a time) |

Its own tests are `tests/unit/test_eval_*.py` and run with the rest of `uv run pytest`.
