"""The scope guard's decision type and interface."""

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal, Protocol

from agent.data_models import HistoryMessage

ScopeCategory = Literal[
    "own_data",
    "manage_budgets_rules_goals",
    "money_education",
    "assistant_chitchat",
    "off_topic",
    "personalized_advice",
    "manipulation",
]
ALLOWED_CATEGORIES: frozenset[str] = frozenset(
    {"own_data", "manage_budgets_rules_goals", "money_education", "assistant_chitchat"}
)
Layer = Literal["prefilter", "classifier"]


class ScopeCheckError(Exception):
    """The scope check could not reach a decision. The chat must not answer."""


@dataclass(frozen=True)
class ScopeDecision:
    """Which category a message falls in, and which layer decided."""

    category: ScopeCategory
    layer: Layer

    @property
    def allowed(self) -> bool:
        """True when the main agent may answer."""
        return self.category in ALLOWED_CATEGORIES


class ScopeGuard(Protocol):
    """Decides whether the latest message is in scope. Raises `ScopeCheckError` on failure."""

    def check(self, message: str, history: Sequence[HistoryMessage]) -> ScopeDecision:
        """Return the decision for `message`, given the earlier turns."""
        ...


class AllowAllGuard:
    """Allows every message. For tests and for drawing the graph; never used by the service."""

    def check(self, message: str, history: Sequence[HistoryMessage]) -> ScopeDecision:
        """Return an allowed decision."""
        return ScopeDecision(category="own_data", layer="classifier")
