import logging
import os
from contextlib import asynccontextmanager

import psycopg
from dotenv import load_dotenv
from fastapi import FastAPI, HTTPException

from agent import AnalyzerAgent, ChatAgent
from agent.data_models import ChatRequest, ChatResponse
from anomaly.data_models import AnomalyRequest, AnomalyResponse
from anomaly.detector import MedianDetector, check_transactions
from db import get_connection

load_dotenv()
logging.basicConfig(level=logging.INFO)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Initialize and clean up shared application resources."""
    openai_key = os.getenv("OPENAI_API_KEY")
    model = os.getenv("AGENT_MODEL", "gpt-4.1")

    if openai_key:
        app.state.analyzer_agent = AnalyzerAgent().create(openai_key, model)
        app.state.chat_agent = ChatAgent().create(openai_key, model)
    else:
        logging.warning("OPENAI_API_KEY not set; /chat and /analyze are disabled.")
        app.state.analyzer_agent = None
        app.state.chat_agent = None

    app.state.detector = MedianDetector(multiplier=float(os.getenv("ANOMALY_MULTIPLIER", "3")))
    yield

    # Cleanup here if we need to


app = FastAPI(lifespan=lifespan)


@app.post("/chat")
def chat(request: ChatRequest) -> ChatResponse:
    """Send a chat request to the configured AI agent."""
    if app.state.chat_agent is None:
        raise HTTPException(status_code=503, detail="AI assistant is not available.")
    return app.state.chat_agent.generate_answer(request)


@app.post("/analyze")
def analyze(request: ChatRequest) -> ChatResponse:
    """Generate a response to a standard chat request."""
    if app.state.analyzer_agent is None:
        raise HTTPException(status_code=503, detail="AI assistant is not available.")
    return app.state.analyzer_agent.generate_answer(request)


@app.post("/anomaly")
def anomaly(request: AnomalyRequest) -> AnomalyResponse:
    """Check a user's new transactions and return the ones that look unusual."""
    return check_transactions(request, app.state.detector)


@app.get("/health")
def health():
    """Return the health status of the service and its database."""
    try:
        with get_connection() as conn:
            conn.execute("SELECT 1")
        database = "ok"
    except psycopg.OperationalError:
        database = "unreachable"
    return {"status": "ok", "database": database}
