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
      3. `/analyze` - POST only (405 in a browser).
      4. `/anomaly` - POST only (405 in a browser).
      5. `/docs` - Shows API documentation. **Use 'Try it out' here** to test the POST routes (`/chat`, `/analyze`, `/anomaly`).
4. Test using `pytest`

## API
   POST /chat
   Request:  {"user_id": 1, "message": "How much did I spend on food?"}
   Response: {"response": "..."}   (answered by the LLM; needs OPENAI_API_KEY)

   POST /analyze
   Request:  {"user_id": 1, "message": "Is a $900 charge at Best Buy suspicious?"}
   Response: {"response": "..."}   (LLM analysis of the transaction; needs OPENAI_API_KEY)

   POST /anomaly
   Request:  {"user_id": 1, "transaction_ids": ["txn_abc", "txn_def"]}
   Response: {"flagged": [{"transaction": {"transaction_id": "txn_abc", "name": "TV", "amount": 900.0, "date": "2026-10-04"},
                           "reason": "$900.00 is over 3x your typical purchase of $15.00"}]}
             (empty list if nothing looks unusual; placeholder rule for now)

   GET /health
   Response: {"status": "ok", "database": "ok"}   ("unreachable" if Postgres is down)