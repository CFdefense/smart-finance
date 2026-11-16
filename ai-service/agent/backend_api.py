"""Call the existing Rust `/api/*` routes on behalf of one verified user."""

import logging
import os
import time
from collections.abc import Callable
from typing import Any, Protocol
from urllib.parse import quote

import httpx

from agent.auth_cookie import COOKIE_NAME, mint_auth_cookie

_log = logging.getLogger("agent.run.backend")

DEFAULT_BASE_URL = "http://127.0.0.1:3001"
CONFLICT_MESSAGE = "That already exists. A budget for that category may already be set."
UNREACHABLE_MESSAGE = "The budgeting service could not be reached."
UNAVAILABLE_MESSAGE = "Saving changes is unavailable right now."
_STATUS_MESSAGES = {
    400: "The budgeting service rejected it as invalid.",
    401: "You are not signed in to the budgeting service.",
    404: "That item was not found.",
    409: CONFLICT_MESSAGE,
}
_SERVER_ERROR = "Something went wrong in the budgeting service."


class BackendError(Exception):
    """A backend call failed. The message is short and safe to show the user."""


class BackendApi(Protocol):
    """Whatever can carry out a confirmed change."""

    def send(self, user_id: int, method: str, path: str, body: dict[str, Any] | None) -> None:
        """Run one request as `user_id`. Raise `BackendError` if it did not succeed."""
        ...


class HttpBackendApi:
    """Calls the Rust API with a cookie minted for the request's verified user."""

    def __init__(
        self,
        base_url: str,
        key_hex: str,
        *,
        client: httpx.Client | None = None,
        clock: Callable[[], float] = time.time,
    ):
        mint_auth_cookie(0, key_hex, 0)  # fail fast on a malformed key
        self._base = base_url.rstrip("/")
        self._key = key_hex
        self._client = client or httpx.Client(timeout=10)
        self._clock = clock

    def send(self, user_id: int, method: str, path: str, body: dict[str, Any] | None) -> None:
        """Call the route as `user_id`; map failures to short messages."""
        cookie = mint_auth_cookie(user_id, self._key, int(self._clock()))
        headers = {"Cookie": f"{COOKIE_NAME}={quote(cookie, safe='')}"}
        try:
            response = self._client.request(
                method, f"{self._base}{path}", json=body, headers=headers
            )
        except httpx.HTTPError as exc:
            _log.warning("backend.unreachable type=%s", type(exc).__name__)
            raise BackendError(UNREACHABLE_MESSAGE) from None
        _log.info("backend.send method=%s path=%s status=%d", method, path, response.status_code)
        if response.is_success:
            return
        raise BackendError(_STATUS_MESSAGES.get(response.status_code, _SERVER_ERROR))


class UnavailableBackendApi:
    """Stands in when the cookie key is missing or malformed."""

    def send(self, user_id: int, method: str, path: str, body: dict[str, Any] | None) -> None:
        """Always refuse."""
        raise BackendError(UNAVAILABLE_MESSAGE)


def backend_from_env() -> BackendApi:
    """Build the HTTP client from `COOKIE_KEY` and `BACKEND_API_URL`, or a refusing stand-in."""
    key = os.environ.get("COOKIE_KEY")
    if not key:
        _log.warning("backend.disabled reason=no_cookie_key")
        return UnavailableBackendApi()
    try:
        return HttpBackendApi(os.environ.get("BACKEND_API_URL", DEFAULT_BASE_URL), key)
    except ValueError:
        _log.warning("backend.disabled reason=bad_cookie_key")
        return UnavailableBackendApi()
