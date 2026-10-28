"""SMTP-based email sender using Python's built-in smtplib."""

import os
import smtplib
from email.message import EmailMessage


class SmtpEmailSender:
    """Send email via SMTP using credentials from environment variables."""

    def send(self, to: str, subject: str, body: str) -> None:
        """Send a plain-text email to `to` via the configured SMTP server."""
        host = os.environ["SMTP_HOST"]
        port = int(os.environ.get("SMTP_PORT", "587"))
        user = os.environ["SMTP_USER"]
        password = os.environ["SMTP_PASSWORD"]
        from_address = os.environ.get("SMTP_FROM_EMAIL", user)

        msg = EmailMessage()
        msg["From"] = from_address
        msg["To"] = to
        msg["Subject"] = subject
        msg.set_content(body)

        with smtplib.SMTP(host, port) as smtp:
            smtp.starttls()
            smtp.login(user, password)
            smtp.send_message(msg)
