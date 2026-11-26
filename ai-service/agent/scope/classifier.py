"""The scope classifier: a separate small model that labels the latest message."""

import logging
import os
from collections.abc import Sequence
from typing import Any

from langchain_openai import ChatOpenAI
from pydantic import BaseModel, SecretStr

from agent.data_models import HistoryMessage
from agent.logging import BASE
from agent.prompts.scope import SCOPE_SYSTEM_PROMPT
from agent.safety import fence
from agent.scope.guard import ScopeCategory, ScopeCheckError, ScopeDecision
from agent.scope.prefilter import MAX_MESSAGE_CHARS, prefilter

_log = logging.getLogger(f"{BASE}.scope")

DEFAULT_SCOPE_MODEL = "gpt-4.1-mini"
HISTORY_TURNS = 4
TURN_CHARS = 300
# One retry: worst case is about 11 s (5 + 5 + backoff), under the 15 s idle timeout in
# src/chat/stream.rs.
TIMEOUT_SECONDS = 5


class ScopeVerdict(BaseModel):
    """The classifier's structured answer."""

    category: ScopeCategory
    reason: str


def build_material(message: str, history: Sequence[HistoryMessage]) -> str:
    """Wrap the recent turns and the latest message as fenced, untrusted data."""
    parts = [
        fence(
            "earlier_user_message" if turn.role == "user" else "earlier_assistant_message",
            turn.content,
            TURN_CHARS,
        )
        for turn in history[-HISTORY_TURNS:]
    ]
    parts.append(fence("latest_user_message", message, MAX_MESSAGE_CHARS))
    return "\n".join(parts)


class ClassifierScopeGuard:
    """Pre-filter first, then the classifier model. Fails closed with `ScopeCheckError`."""

    def __init__(self, llm: Any):
        """`llm` is a chat model with structured output returning a `ScopeVerdict`."""
        self._llm = llm

    def check(self, message: str, history: Sequence[HistoryMessage]) -> ScopeDecision:
        """Return the decision for `message`, or raise `ScopeCheckError` if there is none."""
        early = prefilter(message)
        if early is not None:
            return early
        try:
            verdict = self._llm.invoke(
                [
                    ("system", SCOPE_SYSTEM_PROMPT),
                    ("human", build_material(message, history)),
                ]
            )
        except Exception as exc:
            _log.error("scope.classifier.failed type=%s", type(exc).__name__)
            raise ScopeCheckError("the scope classifier failed") from exc
        if not isinstance(verdict, ScopeVerdict):
            _log.error("scope.classifier.unparseable")
            raise ScopeCheckError("the scope classifier returned no verdict")
        return ScopeDecision(category=verdict.category, layer="classifier")


def default_scope_guard(openai_key: str) -> ClassifierScopeGuard:
    """Build the production guard. `SCOPE_MODEL` picks the model (default `gpt-4.1-mini`)."""
    llm = ChatOpenAI(
        api_key=SecretStr(openai_key),
        model=os.getenv("SCOPE_MODEL") or DEFAULT_SCOPE_MODEL,
        temperature=0,
        timeout=TIMEOUT_SECONDS,
        max_retries=1,
    )
    return ClassifierScopeGuard(llm.with_structured_output(ScopeVerdict))
