"""Test doubles shared by the unit tests and the eval runner."""

from typing import Any

from langchain_core.callbacks import BaseCallbackHandler

from agent.backend_api import CONFLICT_MESSAGE, UNREACHABLE_MESSAGE, BackendError
from agent.pending import PendingActionStore


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


class ToolTrace(BaseCallbackHandler):
    """Records every tool the agent calls, with its arguments and the turn it happened in."""

    def __init__(self):
        self.turn = 0
        self.calls: list[dict[str, Any]] = []

    def reset(self) -> None:
        """Forget recorded calls and go back to turn 0."""
        self.turn = 0
        self.calls = []

    def on_tool_start(
        self, serialized: dict[str, Any], input_str: str, *, inputs: dict | None = None, **kwargs
    ) -> None:
        """Record one tool call. The injected graph state is dropped: it is not an argument."""
        args = {key: value for key, value in (inputs or {}).items() if key != "state"}
        self.calls.append({"turn": self.turn, "name": serialized.get("name", ""), "args": args})


class Rig:
    """The fakes one eval process shares: backend recorder, tool trace and pending actions."""

    def __init__(self):
        self.backend = RecordingBackendApi()
        self.trace = ToolTrace()
        self.pending = PendingActionStore()

    def reset(self, mode: str = "ok") -> None:
        """Prepare a clean test: no calls, no pending changes, turn 0, backend in `mode`."""
        self.backend.reset(mode)
        self.trace.reset()
        self.pending.clear()

    def set_turn(self, turn: int) -> None:
        """Tell both records which user message is being handled."""
        self.backend.turn = turn
        self.trace.turn = turn
