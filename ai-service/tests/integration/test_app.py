from fastapi.testclient import TestClient

from agent.data_models import ChatResponse
from main import app

client = TestClient(app)


def test_health_returns_ok():
    """Verify the health endpoint returns an OK status."""
    response = client.get("/health")

    assert response.status_code == 200
    assert response.json() == {"status": "ok", "database": "ok"}


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
