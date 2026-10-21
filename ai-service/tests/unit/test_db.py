from fastapi.testclient import TestClient

from main import app

client = TestClient(app)


def test_health_reports_database_unreachable(monkeypatch):
    """Verify the health endpoint reports database unreachable."""
    monkeypatch.setenv("DATABASE_URL", "postgresql://nobody@127.0.0.1:1/nothing")

    response = client.get("/health")

    assert response.json()["database"] == "unreachable"
