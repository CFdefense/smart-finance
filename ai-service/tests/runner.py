"""Run the analyze or chat agent in-process against an eval database."""

import os
import tempfile
import uuid
from collections.abc import Iterable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import date, timedelta
from typing import Any
from unittest.mock import patch

import httpx
from dotenv import find_dotenv, load_dotenv

from agent import (
    AnalyzedTransaction,
    AnalyzerAgent,
    AnalyzeRequest,
    ChatAgent,
    ChatRequest,
)
from agent.data_models import HistoryMessage
from anomaly import (
    CombinedDetector,
    DuplicateDetector,
    SubscriptionIncreaseDetector,
    ZScoreDetector,
)
from definition import EvalCase, SearchResultDef
from evaldb import resolve_account
from rig import Rig

_agents: dict[str, object] = {}
# One recorder, trace and pending-action store for the process; the chat agent is built around them.
RIG = Rig()


@dataclass(frozen=True)
class RunResult:
    """What one eval run produced: the text to grade and a record of what the agent did."""

    output: str
    tool_calls: list[dict[str, Any]]
    backend_calls: list[dict[str, Any]]


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
            _agents[kind] = ChatAgent(
                key,
                model,
                backend_api=RIG.backend,
                pending=RIG.pending,
                extra_callbacks=[RIG.trace],
            )
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


class _StubResponse:
    """A Tavily response that always succeeds."""

    def __init__(self, payload: dict[str, Any]):
        self._payload = payload

    def raise_for_status(self) -> None:
        """Succeed."""

    def json(self) -> dict[str, Any]:
        """Return the stubbed results."""
        return self._payload


@contextmanager
def stubbed_search(results: Iterable[SearchResultDef] | None) -> Iterator[None]:
    """Answer web searches with exactly `results`. `None` leaves the real web alone."""
    if results is None:
        yield
        return
    payload = {"results": [{"title": r.title, "url": r.url, "content": r.content} for r in results]}

    def post(*args: Any, **kwargs: Any) -> _StubResponse:
        return _StubResponse(payload)

    previous = os.environ.get("TAVILY_API_KEY")
    os.environ["TAVILY_API_KEY"] = "eval-stub"
    try:
        with patch.object(httpx, "post", post):
            yield
    finally:
        if previous is None:
            os.environ.pop("TAVILY_API_KEY", None)
        else:
            os.environ["TAVILY_API_KEY"] = previous


def _run_chat(case: EvalCase, user_id: int) -> RunResult:
    """Send the message and any follow-ups, feeding each real reply back as history."""
    agent = _agent("chat")
    RIG.reset(case.backend)
    history: list[HistoryMessage] = []
    replies: list[str] = []
    with stubbed_search(case.search_results):
        for turn, message in enumerate([case.message, *case.followups]):
            RIG.set_turn(turn)
            request = ChatRequest(user_id=user_id, message=message, history=list(history))
            reply = agent.generate_answer(request).response
            history += [
                HistoryMessage(role="user", content=message),
                HistoryMessage(role="assistant", content=reply),
            ]
            replies.append(reply)
    if len(replies) == 1:
        output = replies[0]
    else:
        output = "\n\n".join(f"[Turn {i}] {reply}" for i, reply in enumerate(replies, start=1))
    return RunResult(output, list(RIG.trace.calls), list(RIG.backend.calls))


def run(case: EvalCase, db_url: str, user_id: int, accounts: dict[str, str]) -> RunResult:
    """Run the case's agent as `user_id` against `db_url` and return what it said and did."""
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
            return RunResult(_agent("analyze").analyze(request).model_dump_json(indent=2), [], [])
        return _run_chat(case, user_id)
