from dataclasses import dataclass


@dataclass(frozen=True)
class NotificationPreferences:
    """Notification preferences fetched from the database for one user."""

    email: str
    email_notifications: bool
    phone_number: str | None
    sms_notifications: bool
    sms_carrier_gateway: str | None
