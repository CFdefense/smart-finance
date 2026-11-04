"""Fetch notification preferences and dispatch SMS / email for a user."""

import logging
from typing import Protocol

from db import load_preferences
from notifications.providers import SmtpEmailSender


class EmailSender(Protocol):
    """Anything that can send an email to an address."""

    def send(self, to: str, subject: str, body: str) -> None:
        """Send an email with `subject` and `body` to `to`."""
        ...


logger = logging.getLogger(__name__)

# Module-level sender — swap out in tests via monkeypatch.
# Env vars are read lazily inside send().
email_sender: EmailSender = SmtpEmailSender()


def send_notification(user_id: int, message: str) -> None:
    """
    Send `message` to `user_id` via every channel they have enabled.

    Fetches the user's notification preferences from the database, then
    dispatches an email and/or an email-to-SMS message according to those
    preferences. Channels that are disabled are silently skipped.
    """
    prefs = load_preferences(user_id)
    dispatched = False

    if prefs.email_notifications:
        email_sender.send(to=prefs.email, subject="Smart Finance Notification", body=message)
        logger.info("Email sent to user %d", user_id)
        dispatched = True

    if prefs.sms_notifications and prefs.phone_number and prefs.sms_carrier_gateway:
        # Strip the leading '+' and country code prefix that Plaid stores in E.164 format,
        # keeping only the 10-digit national number for US carrier gateways.
        digits = prefs.phone_number.lstrip("+")
        if digits.startswith("1"):
            digits = digits[1:]
        gateway_address = f"{digits}@{prefs.sms_carrier_gateway}"
        email_sender.send(to=gateway_address, subject="", body=message)
        logger.info("SMS (via email gateway) sent to user %d", user_id)
        dispatched = True

    if not dispatched:
        logger.info("No active notification channels for user %d — message not sent", user_id)
