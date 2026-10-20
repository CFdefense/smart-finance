from fastapi.testclient import TestClient
from main import app, generate_answer

client = TestClient(app)

def test_health_returns_ok():
    response = client.get('/health')
    
    assert response.status_code == 200
    assert response.json() == {'status': 'ok'}

def test_chat_returns_an_answer():
    response = client.post("/chat", json={"user_id": 1, "message": "Food spending?"})

    assert response.status_code == 200
    assert isinstance(response.json()["response"], str)
    
def test_chat_rejects_non_integer_user_id():
    response = client.post("/chat", json={"user_id": "abc", "message": "Food spending?"})
    assert response.status_code == 422


def test_chat_rejects_missing_user_id():
    response = client.post("/chat", json={"message": "Food spending?"})
    assert response.status_code == 422


def test_chat_rejects_missing_message():
    response = client.post("/chat", json={"user_id": 1})
    assert response.status_code == 422