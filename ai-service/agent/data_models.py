from datetime import date
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class ChatRequest(BaseModel):
    """Represent an incoming chat request."""

    user_id: int
    message: str


class ChatResponse(BaseModel):
    """Represent a response returned by the AI service."""

    response: str


class Conversation(BaseModel):
    """Represent a user conversation."""

    user_id: int
    history: list[tuple[ChatRequest, ChatResponse]]


Verdict = Literal["OK", "REVIEW", "ALERT"]
AnalysisStatus = Literal["ANALYZED", "DUPLICATE", "FAILED"]


class AnalyzedTransaction(BaseModel):
    """One transaction to analyze. Mirrors the backend's `TransactionResponse`."""

    model_config = ConfigDict(frozen=True)

    transaction_id: str
    account_id: str
    name: str
    amount: float  # positive = money out (Plaid's convention)
    date: date
    pending: bool
    category: str


class AnalyzeRequest(BaseModel):
    """Represent a request to analyze one transaction for one user."""

    user_id: int
    transaction: AnalyzedTransaction


class Assessment(BaseModel):
    """The model's structured judgement about a transaction."""

    verdict: Verdict
    summary: str
    rule_violations: list[str] = Field(default_factory=list)
    budget_notes: list[str] = Field(default_factory=list)
    goal_notes: list[str] = Field(default_factory=list)
    should_notify: bool
    notify_reason: str  # one short sentence


class AnalysisResult(BaseModel):
    """Represent the outcome of analyzing one transaction."""

    transaction_id: str
    status: AnalysisStatus
    assessment: Assessment | None = None
    notified: bool = False
    error: str | None = None
