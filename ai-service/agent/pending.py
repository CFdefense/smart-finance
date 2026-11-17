"""Pending write actions: the model proposes, and only a later user turn can confirm."""

import threading
import time
import uuid
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

DEFAULT_TTL_SECONDS = 600
DEFAULT_CAPACITY = 200

_NOTHING = "There is nothing waiting for confirmation. Propose the change first."
_SEVERAL = "Several changes are waiting: {refs}. Ask the user which one, then use its reference."
_UNKNOWN = "There is no pending change with that reference. It may have expired; propose it again."
_EXPIRED = "That change expired. Propose it again."
_SAME_REQUEST = (
    "A change cannot be confirmed in the same turn it was proposed. "
    "Ask the user to confirm, and wait for their reply."
)


def _target(kind: str, payload: dict[str, Any]) -> tuple[str, str, str] | None:
    """Identify what an action changes: its kind, method and path (None if not a route call)."""
    if "method" not in payload or "path" not in payload:
        return None
    return kind, payload["method"], payload["path"]


class PendingError(Exception):
    """A pending action cannot be confirmed. The message is safe to show the model."""


@dataclass(frozen=True)
class PendingAction:
    """One proposed change: what to send to the backend and how to describe it."""

    action_id: str
    user_id: int
    kind: str
    payload: dict[str, Any]
    preview: str
    request_id: str
    created_at: float


class PendingActionStore:
    """Thread-safe, in-memory, per-user store of proposed changes."""

    def __init__(
        self,
        ttl: float = DEFAULT_TTL_SECONDS,
        capacity: int = DEFAULT_CAPACITY,
        clock: Callable[[], float] = time.monotonic,
    ):
        self._ttl = ttl
        self._capacity = capacity
        self._clock = clock
        self._items: dict[str, PendingAction] = {}
        self._lock = threading.Lock()

    def __len__(self) -> int:
        """Return how many actions are waiting."""
        with self._lock:
            self._purge()
            return len(self._items)

    def add(
        self, user_id: int, kind: str, payload: dict[str, Any], preview: str, request_id: str
    ) -> PendingAction:
        """
        Stage an action for `user_id`, dropping the oldest if the store is full.

        An earlier action of the same user with the same kind, method and path is replaced, so a
        revised proposal ("make it $400") leaves only the latest one to confirm.
        """
        with self._lock:
            self._purge()
            target = _target(kind, payload)
            if target is not None:
                for key in [
                    k
                    for k, a in self._items.items()
                    if a.user_id == user_id and _target(a.kind, a.payload) == target
                ]:
                    del self._items[key]
            while len(self._items) >= self._capacity:
                del self._items[next(iter(self._items))]
            action_id = uuid.uuid4().hex[:8]
            while action_id in self._items:
                action_id = uuid.uuid4().hex[:8]
            action = PendingAction(
                action_id=action_id,
                user_id=user_id,
                kind=kind,
                payload=payload,
                preview=preview,
                request_id=request_id,
                created_at=self._clock(),
            )
            self._items[action.action_id] = action
            return action

    def take(self, user_id: int, request_id: str, action_id: str | None = None) -> PendingAction:
        """
        Remove and return the user's action to run, or raise `PendingError`.

        Without `action_id` the user's only pending action is used. An action proposed in this
        same request is refused and left in place for a later request.
        """
        with self._lock:
            expired = self._purge()
            mine = [a for a in self._items.values() if a.user_id == user_id]
            if action_id is None:
                if not mine:
                    # Only the caller's own expiry is reported; never hint at other users'.
                    raise PendingError(_EXPIRED if user_id in expired else _NOTHING)
                if len(mine) > 1:
                    refs = "; ".join(f"{a.action_id} = {a.preview}" for a in mine)
                    raise PendingError(_SEVERAL.format(refs=refs))
                action = mine[0]
            else:
                wanted = action_id.strip()
                found = next((a for a in mine if a.action_id == wanted), None)
                if found is None:
                    raise PendingError(_UNKNOWN)
                action = found
            if action.request_id == request_id:
                raise PendingError(_SAME_REQUEST)
            del self._items[action.action_id]
            return action

    def clear(self) -> None:
        """Forget every pending action."""
        with self._lock:
            self._items.clear()

    def _purge(self) -> set[int]:
        """Drop expired actions (lock held). Return the user ids whose actions expired."""
        cutoff = self._clock() - self._ttl
        stale = [k for k, a in self._items.items() if a.created_at <= cutoff]
        owners = {self._items[k].user_id for k in stale}
        for key in stale:
            del self._items[key]
        return owners
