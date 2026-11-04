from fastapi.testclient import TestClient

from agent import AnalysisResult, ChatResponse
from main import app

client = TestClient(app)

TRANSACTION = {
    "transaction_id": "t1",
    "account_id": "acc-1",
    "name": "Casino",
    "amount": 250.0,
    "date": "2026-11-04",
    "pending": False,
    "category": "ENTERTAINMENT",
}
ANALYZE_BODY = {"user_id": 7, "transaction": TRANSACTION}


def test_health_returns_ok():
    """Verify the health endpoint returns an OK status."""
    response = client.get("/health")

    assert response.status_code == 200
    assert response.json() == {"status": "ok", "database": "ok"}


def test_service_starts_without_openai_key(monkeypatch):
    """Missing key: the app still starts, AI routes return 503, the detector exists."""
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)

    with TestClient(app) as started:  # 'with' runs lifespan
        chat_response = started.post("/chat", json={"user_id": 1, "message": "Hi"})
        analyzer_response = started.post("/analyze", json=ANALYZE_BODY)

        assert chat_response.status_code == 503
        assert analyzer_response.status_code == 503
        assert app.state.detector is not None


class FakeChatAgent:
    """Stand-in for the real LLM agent so tests never call OpenAI."""

    def generate_answer(self, request):
        """Return a fixed answer."""
        return ChatResponse(response="fake answer")


def test_chat_returns_an_answer(monkeypatch):
    """Verify the chat endpoint returns the agent's response."""
    monkeypatch.setattr(app.state, "chat_agent", FakeChatAgent(), raising=False)

    response = client.post("/chat", json={"user_id": 1, "message": "Food spending?"})

    assert response.status_code == 200
    assert response.json() == {"response": "fake answer"}


def test_chat_returns_503_when_agent_not_configured(monkeypatch):
    """Without an AI agent, /chat and /analyze says it's unavailable instead of crashing."""
    monkeypatch.setattr(app.state, "chat_agent", None, raising=False)
    monkeypatch.setattr(app.state, "analyzer_agent", None, raising=False)

    chat_response = client.post("/chat", json={"user_id": 1, "message": "Hi"})
    analyzer_response = client.post("/analyze", json=ANALYZE_BODY)

    assert chat_response.status_code == 503
    assert analyzer_response.status_code == 503


def test_chat_rejects_non_integer_user_id():
    """Verify the chat endpoint rejects a non-integer user ID."""
    response = client.post(
        "/chat",
        json={"user_id": "abc", "message": "Food spending?"},
    )

    assert response.status_code == 422


def test_chat_rejects_missing_user_id():
    """Verify the chat endpoint rejects a missing user ID."""
    response = client.post(
        "/chat",
        json={"message": "Food spending?"},
    )

    assert response.status_code == 422


def test_chat_rejects_missing_message():
    """Verify the chat endpoint rejects a missing message."""
    response = client.post(
        "/chat",
        json={"user_id": 1},
    )

    assert response.status_code == 422


class FakeAnalyzerAgent:
    """Stand-in for the analyzer so tests never call OpenAI or the database."""

    def __init__(self, result):
        self.result = result
        self.requests = []

    def analyze(self, request):
        """Remember the request and return the prepared result."""
        self.requests.append(request)
        return self.result


def test_analyze_returns_the_agents_result(monkeypatch):
    """The route passes the parsed request to the agent and returns its result."""
    fake = FakeAnalyzerAgent(AnalysisResult(transaction_id="t1", status="DUPLICATE"))
    monkeypatch.setattr(app.state, "analyzer_agent", fake, raising=False)

    response = client.post("/analyze", json=ANALYZE_BODY)

    assert response.status_code == 200
    assert response.json()["status"] == "DUPLICATE"
    assert fake.requests[0].user_id == 7
    assert fake.requests[0].transaction.pending is False


def test_analyze_failed_run_is_still_200(monkeypatch):
    """A degraded run is reported in the body so the caller can decide whether to retry."""
    fake = FakeAnalyzerAgent(
        AnalysisResult(transaction_id="t1", status="FAILED", error="assess_failed")
    )
    monkeypatch.setattr(app.state, "analyzer_agent", fake, raising=False)

    response = client.post("/analyze", json=ANALYZE_BODY)

    assert response.status_code == 200
    assert response.json()["error"] == "assess_failed"


def test_analyze_rejects_an_account_the_user_does_not_own(monkeypatch):
    """An unowned account is a client error, not a server failure."""
    fake = FakeAnalyzerAgent(
        AnalysisResult(transaction_id="t1", status="FAILED", error="account_not_owned")
    )
    monkeypatch.setattr(app.state, "analyzer_agent", fake, raising=False)

    response = client.post("/analyze", json=ANALYZE_BODY)

    assert response.status_code == 422


def test_analyze_rejects_the_old_free_text_body():
    """The previous {user_id, message} body is no longer accepted."""
    response = client.post("/analyze", json={"user_id": 1, "message": "Hi"})

    assert response.status_code == 422


def test_analyze_rejects_a_malformed_transaction():
    """A transaction missing required fields is rejected before the agent runs."""
    response = client.post("/analyze", json={"user_id": 1, "transaction": {"name": "x"}})

    assert response.status_code == 422
