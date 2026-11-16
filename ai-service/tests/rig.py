"""Test doubles shared by the unit tests and the eval runner."""

from typing import Any

from agent.backend_api import CONFLICT_MESSAGE, UNREACHABLE_MESSAGE, BackendError


class RecordingBackendApi:
    """A BackendApi that records every call. `mode` is ok, conflict (409) or down."""

    def __init__(self, mode: str = "ok"):
        self.mode = mode
        self.turn = 0
        self.calls: list[dict[str, Any]] = []

    def reset(self, mode: str = "ok") -> None:
        """Forget recorded calls, go back to turn 0 and switch mode."""
        self.mode = mode
        self.turn = 0
        self.calls = []

    def send(self, user_id: int, method: str, path: str, body: dict[str, Any] | None) -> None:
        """Record the call, then succeed or fail according to the mode."""
        self.calls.append(
            {"turn": self.turn, "user_id": user_id, "method": method, "path": path, "body": body}
        )
        if self.mode == "conflict":
            raise BackendError(CONFLICT_MESSAGE)
        if self.mode == "down":
            raise BackendError(UNREACHABLE_MESSAGE)
