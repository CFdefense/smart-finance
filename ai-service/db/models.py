"""Domain models: rows loaded from the database, shared by every feature."""

from datetime import date

from pydantic import BaseModel, ConfigDict


class Transaction(BaseModel):
    """One transaction. Positive amounts are money going out (Plaid's convention)."""

    transaction_id: str
    name: str
    amount: float
    date: date
    # Every stored transaction has a category ('OTHER' when uncategorized). It's optional
    # here so callers and tests can build transactions without one.
    category: str | None = None


class Account(BaseModel):
    """One linked bank account."""

    account_id: str
    name: str
    account_type: str
    account_subtype: str | None = None
    current_balance: float | None = None
    available_balance: float | None = None


class Budget(BaseModel):
    """An active monthly budget and how much has been spent against it this month."""

    name: str
    category: str
    monthly_limit: float
    spent_this_month: float


class Goal(BaseModel):
    """An account-balance goal that hasn't been archived."""

    name: str
    description: str | None = None
    account_name: str
    target_amount: float
    baseline_balance: float
    current_balance: float | None = None
    target_date: date | None = None


class NotificationPreferences(BaseModel):
    """How one user wants to be notified."""

    model_config = ConfigDict(frozen=True)

    email: str
    email_notifications: bool
    phone_number: str | None
    sms_notifications: bool
    sms_carrier_gateway: str | None
