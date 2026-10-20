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
      1. `/health`
      2. `/chat`
      3. `/docs`