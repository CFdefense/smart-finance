"""Unit tests for SmtpEmailSender."""

from email.message import EmailMessage
from unittest.mock import MagicMock, patch

import pytest

from notifications.providers import SmtpEmailSender

SMTP_ENV = {
    "SMTP_HOST": "smtp.example.com",
    "SMTP_PORT": "587",
    "SMTP_USER": "sender@example.com",
    "SMTP_PASSWORD": "secret",
    "SMTP_FROM_EMAIL": "noreply@example.com",
}


@pytest.fixture()
def mock_smtp():
    """Patch smtplib.SMTP with a MagicMock and return the instance."""
    with patch("notifications.providers.smtplib.SMTP") as smtp_cls:
        instance = MagicMock()
        smtp_cls.return_value.__enter__ = MagicMock(return_value=instance)
        smtp_cls.return_value.__exit__ = MagicMock(return_value=False)
        yield smtp_cls, instance


def test_send_opens_correct_host_and_port(mock_smtp, monkeypatch):
    """SMTP is opened with the host and port from env vars."""
    smtp_cls, _ = mock_smtp
    for k, v in SMTP_ENV.items():
        monkeypatch.setenv(k, v)

    SmtpEmailSender().send(to="u@example.com", subject="Hi", body="Hello")

    smtp_cls.assert_called_once_with("smtp.example.com", 587)


def test_send_calls_starttls_login_send(mock_smtp, monkeypatch):
    """starttls, login, and send_message are all called in order."""
    _, instance = mock_smtp
    for k, v in SMTP_ENV.items():
        monkeypatch.setenv(k, v)

    SmtpEmailSender().send(to="u@example.com", subject="Hi", body="Hello")

    instance.starttls.assert_called_once()
    instance.login.assert_called_once_with("sender@example.com", "secret")
    instance.send_message.assert_called_once()


def test_send_message_has_correct_headers(mock_smtp, monkeypatch):
    """The EmailMessage sent has the right From, To, and Subject headers."""
    _, instance = mock_smtp
    for k, v in SMTP_ENV.items():
        monkeypatch.setenv(k, v)

    SmtpEmailSender().send(to="dest@example.com", subject="Test subject", body="Body text")

    sent_msg: EmailMessage = instance.send_message.call_args[0][0]
    assert sent_msg["From"] == "noreply@example.com"
    assert sent_msg["To"] == "dest@example.com"
    assert sent_msg["Subject"] == "Test subject"


def test_send_message_body(mock_smtp, monkeypatch):
    """The EmailMessage body contains the text passed in."""
    _, instance = mock_smtp
    for k, v in SMTP_ENV.items():
        monkeypatch.setenv(k, v)

    SmtpEmailSender().send(to="u@example.com", subject="", body="Important alert")

    sent_msg: EmailMessage = instance.send_message.call_args[0][0]
    assert "Important alert" in sent_msg.get_content()


def test_smtp_from_email_defaults_to_smtp_user(mock_smtp, monkeypatch):
    """When SMTP_FROM_EMAIL is absent, the From address falls back to SMTP_USER."""
    _, instance = mock_smtp
    env = {k: v for k, v in SMTP_ENV.items() if k != "SMTP_FROM_EMAIL"}
    for k, v in env.items():
        monkeypatch.setenv(k, v)
    monkeypatch.delenv("SMTP_FROM_EMAIL", raising=False)

    SmtpEmailSender().send(to="u@example.com", subject="Hi", body="Hello")

    sent_msg: EmailMessage = instance.send_message.call_args[0][0]
    assert sent_msg["From"] == "sender@example.com"


def test_default_port_is_587(mock_smtp, monkeypatch):
    """When SMTP_PORT is absent, port 587 is used."""
    smtp_cls, _ = mock_smtp
    env = {k: v for k, v in SMTP_ENV.items() if k != "SMTP_PORT"}
    for k, v in env.items():
        monkeypatch.setenv(k, v)
    monkeypatch.delenv("SMTP_PORT", raising=False)

    SmtpEmailSender().send(to="u@example.com", subject="Hi", body="Hello")

    smtp_cls.assert_called_once_with("smtp.example.com", 587)
