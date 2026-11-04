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
   May search the web with Tavily, which needs TAVILY_API_KEY (without it, chat still works but search fails gracefully).

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

## Agent logging

Both agents (chat and analyzer) log under `agent.run.*`. Set `AGENT_LOG_LEVEL` (default `INFO`).
INFO shows each run's workflow: node enter/exit with timings, routing decisions, tool calls and
LLM calls (plus the notify decision for the analyzer). Every line is tagged
`[run=<id> txn=<transaction_id> user=<id>]` (chat has no `txn`) so `grep run=<id>` follows one
run. INFO never contains user text, search text or exception messages. DEBUG adds prompts, raw
tool results, tool arguments, notification text and the graph as Mermaid.

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
