# AI Service 

This is the ai agent service. It takes http requests and returns LLM responses. 

## Run Instructions

1. Enter the ai-service folder: `cd ai-service`
2. Set up the virtual environment

```bash
python3 -m venv .venv
pip3 install fastapi uvicorn
```
3. Start the HTTP service: `uvicorn main:app --reload`
4. View the port by the following into your browser searchbar: `http://127.0.0.1:8000/health`