import logging
import os
from contextlib import asynccontextmanager

import psycopg
from dotenv import load_dotenv
from fastapi import FastAPI

from agent import AnalyzerAgent, ChatAgent
from agent.data_models import ChatRequest, ChatResponse
from db import get_connection
from monitor.data_models import AnomalyRequest, AnomalyResponse
from monitor.workers import job_queue, submit_check

load_dotenv()
logging.basicConfig(level=logging.INFO)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Initialize and clean up shared application resources."""
    app.state.openai_key = os.environ["OPENAI_API_KEY"]
    app.state.model = os.getenv("AGENT_MODEL", "gpt-4.1")
    app.state.analyzer_agent = AnalyzerAgent().create(app.state.openai_key, app.state.model)
    app.state.chat_agent = ChatAgent().create(app.state.openai_key, app.state.model)

    yield

    # Cleanup here if we need to
    job_queue.shutdown(wait=True)


app = FastAPI(lifespan=lifespan)


@app.post("/chat")
def chat(request: ChatRequest) -> ChatResponse:
    """Send a chat request to the configured AI agent."""
    return app.state.chat_agent.generate_answer(request)


@app.post("/analyze")
def analyze(request: ChatRequest) -> ChatResponse:
    """Generate a response to a standard chat request."""
    return app.state.analyzer_agent.generate_answer(request)


@app.post("/anomaly")
def anomaly(request: AnomalyRequest) -> AnomalyResponse:
    """Check a user's new transactions and return the ones that look unusual."""
    flagged = submit_check(request.user_id, request.transaction_ids)
    return AnomalyResponse(flagged=flagged)


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
