# AI Service 

This is the ai agent service. It takes http requests and returns LLM responses. 

## Run Instructions

1. Setup folder

```bash
cd ai-service
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

2. Start the HTTP service: `uvicorn main:app --reload`
3. View the port by the following into your browser searchbar: `http://127.0.0.1:8000/<route>`
   1. Valid routes:
      1. `/health` - Get HTTP API health. 
      2. `/chat` - Opening this will result in a 405 error in browser, since it is a POST and not a GET. 
      3. `/docs` - Shows API skeleton documentation. **Use 'Try it out' here** to test `/chat` input and output.

## API
   POST /chat
   Request:  {"user_id": 1, "message": "How much did I spend on food?"}
   Response: {"response": "..."}   (placeholder text for now)