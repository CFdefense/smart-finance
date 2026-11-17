# AI Service 

This is the ai agent service. It takes http requests and returns LLM responses. 

## Run Instructions

1. Setup folder

```bash
cd ai-service
uv sync
```

2. Start the HTTP service: `uvicorn main:app --reload`
3. View the service by typing the following into your browser's address bar: `http://127.0.0.1:8000/<route>`
   1. Valid routes:
      1. `/health` - Get HTTP API and database health.
      2. `/chat` - POST only. Opening this in a browser gives a 405 error, since browsers send GET.
      3. `/analyze` - POST only (405 in a browser). Takes a transaction to analyze.
      4. `/anomaly` - POST only (405 in a browser).
      5. `/docs` - Shows API documentation. **Use 'Try it out' here** to test the POST routes (`/chat`, `/analyze`, `/anomaly`).
4. Test using `pytest`. To evaluate the agents themselves with LLM graders, see `tests/README.md`.

## API
   POST /chat
   Request:  {"user_id": 1, "message": "How much did I spend on food?",
              "history": [{"role": "user", "content": "..."}, {"role": "assistant", "content": "..."}]}
             history is optional (at most 20 earlier messages, oldest first).
   Response: {"response": "..."}   (answered by the LLM; needs OPENAI_API_KEY)
   May search the web with Tavily, which needs TAVILY_API_KEY (without it, chat still works but search fails gracefully).
   Can also create, change and delete budgets, rules and goals; see "What the chat agent can do" below.

   POST /analyze
   Request:  {"user_id": 1,
              "transaction": {"transaction_id": "txn_abc", "account_id": "acc_1", "name": "Best Buy",
                              "amount": 900.0, "date": "2026-11-04", "pending": false,
                              "category": "SHOPPING"}}
   Response: {"transaction_id": "txn_abc", "status": "ANALYZED",
              "assessment": {"verdict": "REVIEW", "summary": "...", "rule_violations": [],
                             "budget_notes": [], "goal_notes": [], "should_notify": true,
                             "notify_reason": "..."},
              "notified": true, "anomaly_flagged": false, "anomaly_reason": null, "error": null}
              anomaly_flagged is null when detection failed or didn't run.
             status is ANALYZED, DUPLICATE (already analyzed) or FAILED (see error).
             422 if the account is not the user's; 503 without OPENAI_API_KEY.
             A FAILED result is still HTTP 200 (retry later); only an unowned account is 422.
             Notifications: OK never notifies, ALERT always does, REVIEW follows the model.

   POST /anomaly
   Request:  {"user_id": 1, "transaction_ids": ["txn_abc", "txn_def"]}
   Response: {"flagged": [{"transaction": {"transaction_id": "txn_abc", "name": "TV", "amount": 900.0, "date": "2026-10-04"},
                           "reason": "$900.00 is over 3x your typical purchase of $15.00"}]}
             (empty list if nothing looks unusual; placeholder rule for now)

   GET /health
   Response: {"status": "ok", "database": "ok"}   ("unreachable" if Postgres is down)

## What the chat agent can do

The chat agent sees the user's accounts, budgets, goals, rules and recent transactions in its prompt, and
has 19 tools:

| Group | Tools | What they do |
|---|---|---|
| Web | `web_search` | General finance facts (rates, tax rules). Never sent the user's own data. |
| Look up | `list_accounts`, `list_transactions`, `list_budgets`, `list_goals`, `list_rules` | Read the user's own data. Items show their ids. |
| Calculate | `spending_by_category`, `spending_by_merchant`, `compare_periods`, `budget_status`, `goal_progress`, `find_recurring_charges`, `can_i_afford` | Exact totals and checks, computed in code (never by the model). Windows are rolling `days`. |
| Change | `propose_budget`, `propose_rule`, `propose_goal`, `propose_update`, `propose_delete`, `confirm_action` | Create, change and delete budgets, rules and goals, **only after the user agrees**. |

### How changes are saved (confirm first)

1. The user asks for a change. The model calls a `propose_*` tool. This validates the request against the
   user's real data and **saves nothing**; it returns a short preview and a reference.
2. The model shows the preview and asks the user to confirm.
3. Only in a **later** chat message, when the user agrees, the model calls `confirm_action`. The service
   refuses a confirmation made in the same request as the proposal, so a prompt cannot propose and confirm in one go.
4. `confirm_action` calls the existing Rust routes (`/api/budgets`, `/api/rules`, `/api/goals`) as that user.
   No new backend routes exist. If the call fails the model is told `NOT saved` and must say so.

Proposals live in memory for 10 minutes, per user, and are single-use. They are lost on a restart, and two
workers do not share them (see "Ideas" below).

### Configuration for saving changes

| Variable | Meaning |
|---|---|
| `COOKIE_KEY` | **Required for saving.** The same 64-byte hex key the Rust server uses. The service mints a short-lived (5 minute) `auth-token` cookie for the request's verified `user_id` only. Without it, chat still reads and searches, and says saving is unavailable. |
| `BIND_ADDRESS` | **Required for saving.** The address the Rust server listens on (for example `127.0.0.1:3001`); the service calls the API there. It is the Rust server's own setting from the shared `.env`, with no default. Without it saving is switched off, as without `COOKIE_KEY`. It must be an address you can connect to, so not `0.0.0.0`. |

`COOKIE_KEY` lets the holder sign in as any user. Keep it out of logs and version control, and give it to the
ai-service only in environments where the service itself is trusted. This is an interim design; see the first
idea below for the cleaner long-term fix.

**Trust boundary.** `/chat` and `/chat/stream` trust the `user_id` in the request body; there is no
authentication of their own. The service must only be reachable from the Rust backend: bind it to loopback
or an internal network and do not expose it publicly. Because the chat can now write, this matters more than
it did when it could only read.

### Known limitations

- A staged change stays confirmable for 10 minutes, for any later request by the same user, regardless of
  chat session.
- Confirming only runs a change the user was shown (the staged preview). Staging a revised change for the
  same item replaces the earlier one.
- The chat history holds only user and assistant text, not tool results, so on the "yes" turn the model
  cannot see the reference it was given. `confirm_action` therefore treats a missing, wrong or invented
  `action_id` as "the user's single waiting change". With several waiting, the user must say which. This
  still cannot happen in the request that made the proposal.
- `goal_progress` paces each goal by the net deposits into the goal's own account over the last 90 days,
  not by all accounts together.
- The model is told to confirm only after an explicit user "yes". That is an instruction to the model, not
  something the service can verify; the service only enforces that confirmation happens in a later request
  than the proposal.

### Ideas for what the chat agent could do next

Roughly in the order we would build them:

1. **A service-to-service auth route in the Rust API** (for example a shared-secret header and an `X-User-Id`),
   so the ai-service no longer needs `COOKIE_KEY`. Removes the biggest security caveat above.
2. **Durable pending changes in Postgres** so confirmations survive restarts and work with several workers.
3. **"What if" simulator**: "what if I cut dining by $100 a month?" projects goal dates and budget room.
4. **Suggest a budget from history**: propose limits from the last three months of real spending, one tap to confirm each.
5. **Cash-flow forecast and bill reminders**: use `find_recurring_charges` to predict the next 30 days and flag a low balance or an upcoming bill.
6. **Savings rate and net-worth trends**: month-by-month charts of money in, money out and total balance.
7. **Anomaly explanations**: "why was this flagged?" answered from the anomaly detectors' reasons.
8. **Re-categorize merchants**: fix a wrong category for one merchant or all past charges, with the same confirm-first flow.
9. **Debt-payoff planner**: snowball versus avalanche for the credit accounts, with a monthly payment budget.
10. **Weekly digest**: a short summary (spending, budgets near the limit, goal progress) sent by email or SMS.
11. **Export**: CSV or PDF of a month's transactions or a category breakdown.
12. **Shared household view**: combine two users' accounts (with both users' consent) for joint budgets.

## How the analyzer gets its data

Every run loads the user's accounts, the rules that apply to the transaction's account, the
monthly budgets and the savings goals up front (the `load_context` node), so the model always
sees all of them. The prompts ask it to check the transaction against every rule, budget and goal
before concluding. Its one tool is `get_transactions`, for comparing with other spending.

## Agent logging

Both agents (chat and analyzer) log under `agent.run.*`. Logging is always on and always verbose
(DEBUG), with payment details. It cannot be turned down.
The logs hold the merchant name, amount, date and category, plus everything the nodes and tools
read and write (the loaded context, tool input and output, the assessment, prompts, notification
text and the graph as Mermaid). Treat the log folder as sensitive. API keys and bearer tokens are
masked as `[redacted]`, including inside tracebacks.

The agents' workflow logs go to the file `ai-service/logs/analyzer.log` (rotating,
5 MB x 3); the terminal keeps only uvicorn's request lines. Follow it with
`tail -f logs/analyzer.log`. Each line reads
`[time] [PID] [LEVEL] [node|graph|tool|llm|decision] [run=<id> txn=<last 4 of id> user=<id>] message`,
so `grep run=<id> logs/analyzer.log` follows one analysis.

Each analysis also writes its own file, `ai-service/logs/log_<transaction_id>.txt`, holding only
that run's lines. Analysing the same transaction again appends to the same file. These per-
transaction files are not rotated or deleted, so clear old ones out now and then. On macOS and
Windows file names are case-insensitive, so ids that differ only in case share one file.

- `AGENT_LOG_DIR`: write the logs somewhere else.

## Drawing the agent graphs

Render an agent's workflow as a PNG, from `ai-service/`:

```
uv run python -m agent analyzer            # writes analyzer_graph.png
uv run python -m agent chat chat.png       # writes chat.png
```

The first argument picks the agent (`analyzer` or `chat`; any unique prefix works, so `analyze`
is fine). The optional second argument is the output file, which defaults to `<agent>_graph.png`
in the current directory. No API key or database is needed, but it needs internet access because
the image is drawn by the mermaid.ink service. From code, use
`agent.graph_image.save_graph_image(graph, path)`.
