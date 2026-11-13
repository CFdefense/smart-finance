"""Run the analyze or chat agent in-process against an eval database."""

import os
import tempfile
import uuid
from contextlib import contextmanager
from datetime import date, timedelta

from dotenv import find_dotenv, load_dotenv

from agent import (
    AnalyzedTransaction,
    AnalyzerAgent,
    AnalyzeRequest,
    ChatAgent,
    ChatRequest,
)
from anomaly import (
    CombinedDetector,
    DuplicateDetector,
    SubscriptionIncreaseDetector,
    ZScoreDetector,
)
from definition import EvalCase
from evaldb import resolve_account

_agents: dict[str, object] = {}


def _agent(kind: str):
    """Return the process-wide agent of this kind, building it on first use."""
    if kind not in _agents:
        load_dotenv(find_dotenv(usecwd=True))
        key = os.getenv("OPENAI_API_KEY")
        if not key:
            raise RuntimeError("OPENAI_API_KEY is not set")
        # Keep per-transaction log files out of ai-service/logs.
        os.environ.setdefault("AGENT_LOG_DIR", tempfile.mkdtemp(prefix="agent-eval-logs-"))
        model = os.getenv("AGENT_MODEL", "gpt-4.1")
        if kind == "analyze":
            detector = CombinedDetector(
                [
                    ZScoreDetector(threshold=float(os.getenv("ANOMALY_ZSCORE_THRESHOLD", "3.5"))),
                    DuplicateDetector(),
                    SubscriptionIncreaseDetector(),
                ]
            )
            _agents[kind] = AnalyzerAgent(key, model, detector)
        else:
            _agents[kind] = ChatAgent(key, model)
    return _agents[kind]


@contextmanager
def database(url: str):
    """Point `DATABASE_URL` at `url` for the duration of the run, then restore it."""
    previous = os.environ.get("DATABASE_URL")
    os.environ["DATABASE_URL"] = url
    try:
        yield
    finally:
        if previous is None:
            os.environ.pop("DATABASE_URL", None)
        else:
            os.environ["DATABASE_URL"] = previous


def run(case: EvalCase, db_url: str, user_id: int, accounts: dict[str, str]) -> str:
    """Run the case's agent as `user_id` against `db_url` and return its response as text."""
    with database(db_url):
        if case.agent == "analyze":
            txn = case.transaction
            request = AnalyzeRequest(
                user_id=user_id,
                transaction=AnalyzedTransaction(
                    transaction_id=f"eval-{uuid.uuid4().hex[:8]}",
                    account_id=resolve_account(accounts, txn.account),
                    name=txn.name,
                    amount=txn.amount,
                    date=date.today() - timedelta(days=txn.days_ago),
                    pending=False,
                    category=txn.category,
                ),
            )
            return _agent("analyze").analyze(request).model_dump_json(indent=2)
        reply = _agent("chat").generate_answer(ChatRequest(user_id=user_id, message=case.message))
        return reply.response
