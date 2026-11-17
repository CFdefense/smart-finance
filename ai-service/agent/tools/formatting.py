"""Small helpers shared by the tools."""

import functools
import logging
from collections.abc import Callable
from datetime import date
from typing import Annotated, Any

from langgraph.prebuilt import InjectedState

from agent.logging import TOOL

MAX_TOOL_CHARS = 4000
MAX_AMOUNT = 1_000_000_000
UNAVAILABLE = "That data is temporarily unavailable. Tell the user, and do not guess."

State = Annotated[dict[str, Any], InjectedState]
"""The graph state a tool receives. The model never sees or supplies it."""

_log = logging.getLogger(TOOL)


class Refusal(Exception):  # noqa: N818 - plan-mandated name
    """A tool declines a request. The message becomes the tool's result for the model."""


def today() -> date:
    """Return today's date (tools call this so tests can pin it)."""
    return date.today()


def cap(text: str) -> str:
    """Cut tool output to MAX_TOOL_CHARS so one tool cannot flood the prompt."""
    if len(text) <= MAX_TOOL_CHARS:
        return text
    return text[:MAX_TOOL_CHARS] + "\n... (truncated)"


def safe_tool(fn: Callable[..., str]) -> Callable[..., str]:
    """
    Make a tool body safe to expose to the model.

    A `Refusal` returns its own message. Any other error returns a generic message and logs only
    the exception type, so database or network details never reach the model or the user.
    Whatever is returned is cut to MAX_TOOL_CHARS.
    """

    @functools.wraps(fn)
    def wrapper(*args: Any, **kwargs: Any) -> str:
        try:
            return cap(fn(*args, **kwargs))
        except Refusal as refusal:
            return cap(str(refusal))
        except Exception as exc:
            _log.warning("tool.failed name=%s type=%s", fn.__name__, type(exc).__name__)
            _log.debug("tool.failed detail", exc_info=True)
            return UNAVAILABLE

    return wrapper
