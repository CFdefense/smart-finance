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
4. Test using `pytest`

## API
   POST /chat
   Request:  {"user_id": 1, "message": "How much did I spend on food?"}
   Response: {"response": "..."}   (answered by the LLM; needs OPENAI_API_KEY)

   POST /analyze
   Request:  {"user_id": 1,
              "transaction": {"transaction_id": "txn_abc", "account_id": "acc_1", "name": "Best Buy",
                              "amount": 900.0, "date": "2026-11-04", "pending": false,
                              "category": "SHOPPING"}}
   Response: {"transaction_id": "txn_abc", "status": "ANALYZED",
              "assessment": {"verdict": "REVIEW", "summary": "...", "rule_violations": [],
                             "budget_notes": [], "goal_notes": [], "should_notify": true,
                             "notify_reason": "..."},
              "notified": true, "error": null}
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

## How the analyzer gets its data

Every run loads the user's accounts, the rules that apply to the transaction's account, the
monthly budgets and the savings goals up front (the `load_context` node), so the model always
sees all of them. The prompts ask it to check the transaction against every rule, budget and goal
before concluding. Its one tool is `get_transactions`, for comparing with other spending.

## Analyzer logging

Logging is always on and always verbose (DEBUG), with payment details. It cannot be turned down.
The logs hold the merchant name, amount, date and category, plus everything the nodes and tools
read and write (the loaded context, tool input and output, the assessment, prompts, notification
text and the graph as Mermaid). Treat the log folder as sensitive. API keys and bearer tokens are
masked as `[redacted]`, including inside tracebacks.

The analyzer's workflow logs go to the file `ai-service/logs/analyzer.log` (rotating,
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

## Trying the analyzer by hand

Scripts in `tests/manual/` (not collected by pytest) set up and drive a local demo. They need the
Postgres from `docker-compose.yml`, the migrations applied, and a real `OPENAI_API_KEY`.

```
cargo run --bin seed-sandbox                          # from the repo root: users and accounts
uv run python tests/manual/seed_christian.py          # budgets, rules, goals, recent spending
uv run uvicorn main:app                               # terminal A
tail -f logs/analyzer.log                             # terminal B: the agent's workflow
uv run python tests/manual/trigger.py list            # terminal C: pick a scenario
uv run python tests/manual/trigger.py bestbuy
```

`trigger.py` also takes `custom --name "Nike" --amount 120 --category SHOPPING`. Each analysis is
recorded in the `transaction_analyses` table by transaction id, so sending the same id again is
not analyzed or notified a second time: `trigger.py duplicate` sends one transaction twice and the
second reply is `DUPLICATE`. Failed analyses and crashed runs (stuck `started` for over 10
minutes) can be retried; finished ones cannot.
