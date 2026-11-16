"""Tests for the client that calls the existing Rust API with a Python-minted cookie."""

import base64
from urllib.parse import unquote

import httpx
import pytest
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from agent.backend_api import (
    CONFLICT_MESSAGE,
    UNAVAILABLE_MESSAGE,
    UNREACHABLE_MESSAGE,
    BackendError,
    HttpBackendApi,
    UnavailableBackendApi,
    backend_from_env,
)
from rig import RecordingBackendApi

KEY = bytes(range(64))
KEY_HEX = KEY.hex()


def make_api(handler, now=1_900_000_000):
    """Build an HttpBackendApi whose HTTP traffic goes to `handler`."""
    client = httpx.Client(transport=httpx.MockTransport(handler))
    return HttpBackendApi("http://api.test", KEY_HEX, client=client, clock=lambda: now)


def decrypt_header(request: httpx.Request) -> str:
    """Return the plaintext of the auth-token cookie on a request."""
    name, _, value = request.headers["cookie"].partition("=")
    assert name == "auth-token"
    raw = base64.b64decode(unquote(value))
    return AESGCM(KEY[32:]).decrypt(raw[:12], raw[12:], b"auth-token").decode()


def test_post_sends_json_and_a_cookie_for_that_user():
    """A create call posts the body to /api/... with a cookie minted for the user."""
    seen = []

    def handler(request):
        seen.append(request)
        return httpx.Response(201)

    make_api(handler).send(7, "POST", "/api/budgets", {"name": "Food", "monthly_limit": 100})

    [request] = seen
    assert request.method == "POST" and str(request.url) == "http://api.test/api/budgets"
    assert request.read() == b'{"name":"Food","monthly_limit":100}'
    assert decrypt_header(request) == "user-7.1900000300.sign"


def test_cookie_is_percent_encoded_for_the_header():
    """Base64 characters like + / = are percent-encoded so the Cookie header is valid."""
    seen = []
    make_api(lambda r: seen.append(r) or httpx.Response(204)).send(
        7, "DELETE", "/api/rules/3", None
    )
    value = seen[0].headers["cookie"].partition("=")[2]
    assert "+" not in value and "/" not in value and "=" not in value


def test_each_call_mints_a_cookie_for_its_own_user():
    """Two users never share a cookie."""
    seen = []
    api = make_api(lambda r: seen.append(r) or httpx.Response(204))
    api.send(1, "DELETE", "/api/goals/1", None)
    api.send(2, "DELETE", "/api/goals/2", None)
    assert decrypt_header(seen[0]).startswith("user-1.")
    assert decrypt_header(seen[1]).startswith("user-2.")


def test_put_and_delete_use_the_right_methods():
    """Update and delete reach the matching routes."""
    seen = []
    api = make_api(lambda r: seen.append(r) or httpx.Response(204))
    api.send(7, "PUT", "/api/budgets/5", {"monthly_limit": 200})
    api.send(7, "DELETE", "/api/budgets/5", None)
    assert [(r.method, r.url.path) for r in seen] == [
        ("PUT", "/api/budgets/5"),
        ("DELETE", "/api/budgets/5"),
    ]


@pytest.mark.parametrize(
    ("status", "fragment"),
    [
        (400, "invalid"),
        (401, "signed in"),
        (404, "not found"),
        (409, CONFLICT_MESSAGE),
        (500, "went wrong"),
        (503, "went wrong"),
    ],
)
def test_error_statuses_become_short_messages(status, fragment):
    """4xx/5xx map to a user-facing message with no backend detail."""
    with pytest.raises(BackendError) as err:
        make_api(lambda r: httpx.Response(status, text="secret internal detail")).send(
            7, "POST", "/api/budgets", {}
        )
    assert fragment in str(err.value)
    assert "secret" not in str(err.value)


def test_network_failure_is_reported_as_unreachable():
    """A connection error becomes the unreachable message."""

    def handler(request):
        raise httpx.ConnectError("boom")

    with pytest.raises(BackendError, match=UNREACHABLE_MESSAGE):
        make_api(handler).send(7, "POST", "/api/budgets", {})


def test_bad_key_is_rejected_at_construction():
    """A malformed COOKIE_KEY fails fast with ValueError."""
    with pytest.raises(ValueError, match="COOKIE_KEY"):
        HttpBackendApi("http://api.test", "xyz")


def test_trailing_slash_in_base_url_is_ignored():
    """http://host/ and http://host behave the same."""
    seen = []
    client = httpx.Client(
        transport=httpx.MockTransport(lambda r: seen.append(r) or httpx.Response(201))
    )
    HttpBackendApi("http://api.test/", KEY_HEX, client=client).send(7, "POST", "/api/rules", {})
    assert str(seen[0].url) == "http://api.test/api/rules"


def test_unavailable_api_always_refuses():
    """Without a usable key every write says saving is unavailable."""
    with pytest.raises(BackendError, match=UNAVAILABLE_MESSAGE):
        UnavailableBackendApi().send(7, "POST", "/api/budgets", {})


def test_backend_from_env_uses_the_environment(monkeypatch):
    """A valid COOKIE_KEY and BACKEND_API_URL give a working HTTP client."""
    monkeypatch.setenv("COOKIE_KEY", KEY_HEX)
    monkeypatch.setenv("BACKEND_API_URL", "http://example.test")
    assert isinstance(backend_from_env(), HttpBackendApi)


def test_backend_from_env_without_a_key_is_unavailable(monkeypatch):
    """No COOKIE_KEY means writes are disabled, not a crash."""
    monkeypatch.delenv("COOKIE_KEY", raising=False)
    assert isinstance(backend_from_env(), UnavailableBackendApi)


def test_backend_from_env_with_a_bad_key_is_unavailable(monkeypatch, caplog):
    """A malformed key disables writes and never logs the key."""
    monkeypatch.setenv("COOKIE_KEY", "not-hex-secret")
    assert isinstance(backend_from_env(), UnavailableBackendApi)
    assert "not-hex-secret" not in caplog.text


def test_default_base_url_is_local(monkeypatch):
    """Without BACKEND_API_URL the local Rust address is used."""
    monkeypatch.setenv("COOKIE_KEY", KEY_HEX)
    monkeypatch.delenv("BACKEND_API_URL", raising=False)
    assert backend_from_env()._base == "http://127.0.0.1:3001"


def test_recording_backend_records_and_succeeds():
    """The eval fake records each call and succeeds in ok mode."""
    rec = RecordingBackendApi()
    rec.send(7, "POST", "/api/budgets", {"name": "x"})
    assert rec.calls == [
        {"turn": 0, "user_id": 7, "method": "POST", "path": "/api/budgets", "body": {"name": "x"}}
    ]


def test_recording_backend_tags_calls_with_the_current_turn():
    """Multi-turn evals set `turn` before each message; reset() returns it to 0."""
    rec = RecordingBackendApi()
    rec.turn = 2
    rec.send(7, "DELETE", "/api/rules/1", None)
    assert rec.calls[0]["turn"] == 2
    rec.reset()
    assert rec.turn == 0 and rec.calls == []


def test_recording_backend_conflict_and_down_modes_raise_after_recording():
    """Conflict and down modes still record the attempt, then fail like the real API."""
    rec = RecordingBackendApi("conflict")
    with pytest.raises(BackendError, match=CONFLICT_MESSAGE):
        rec.send(7, "POST", "/api/budgets", {})
    rec.reset("down")
    assert rec.calls == []
    with pytest.raises(BackendError, match=UNREACHABLE_MESSAGE):
        rec.send(7, "POST", "/api/budgets", {})
    assert len(rec.calls) == 1
