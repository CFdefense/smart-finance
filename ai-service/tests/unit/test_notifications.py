"""Unit tests for the notification dispatcher."""

import pytest

import notifications.dispatcher as dispatcher_module
from notifications.data_models import NotificationPreferences
from notifications.dispatcher import send_notification


class RecordingEmailSender:
    """Records every send() call made to it."""

    def __init__(self):
        self.calls: list[dict] = []

    def send(self, **kwargs):
        """Capture the call arguments."""
        self.calls.append(kwargs)


@pytest.fixture()
def fake_email(monkeypatch):
    """Return a RecordingEmailSender wired in as the module-level email sender."""
    sender = RecordingEmailSender()
    monkeypatch.setattr(dispatcher_module, "email_sender", sender)
    return sender


def _patch_prefs(monkeypatch, prefs: NotificationPreferences):
    monkeypatch.setattr(dispatcher_module, "_load_preferences", lambda user_id: prefs)


# ---------------------------------------------------------------------------
# Email routing
# ---------------------------------------------------------------------------


def test_sends_email_when_enabled(monkeypatch, fake_email):
    """Email is dispatched when email_notifications is True."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=True,
            phone_number=None,
            sms_notifications=False,
            sms_carrier_gateway=None,
        ),
    )

    send_notification(1, "Hello!")

    assert len(fake_email.calls) == 1
    assert fake_email.calls[0] == {
        "to": "u@example.com",
        "subject": "Smart Finance Notification",
        "body": "Hello!",
    }


def test_skips_email_when_disabled(monkeypatch, fake_email):
    """Email is not sent when email_notifications is False."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number=None,
            sms_notifications=False,
            sms_carrier_gateway=None,
        ),
    )

    send_notification(1, "Hello!")

    assert fake_email.calls == []


# ---------------------------------------------------------------------------
# SMS-via-email-gateway routing
# ---------------------------------------------------------------------------


def test_sends_sms_via_gateway_when_enabled(monkeypatch, fake_email):
    """SMS is sent as an email to the carrier gateway address."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number="+15550001234",
            sms_notifications=True,
            sms_carrier_gateway="tmomail.net",
        ),
    )

    send_notification(1, "Alert!")

    assert len(fake_email.calls) == 1
    assert fake_email.calls[0] == {"to": "5550001234@tmomail.net", "subject": "", "body": "Alert!"}


def test_sms_strips_country_code(monkeypatch, fake_email):
    """The +1 country code is stripped before building the gateway address."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number="+15559876543",
            sms_notifications=True,
            sms_carrier_gateway="vtext.com",
        ),
    )

    send_notification(1, "Hi")

    assert fake_email.calls[0]["to"] == "5559876543@vtext.com"


def test_sms_skipped_without_phone_number(monkeypatch, fake_email):
    """SMS is not sent when phone_number is None."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number=None,
            sms_notifications=True,
            sms_carrier_gateway="tmomail.net",
        ),
    )

    send_notification(1, "Hello!")

    assert fake_email.calls == []


def test_sms_skipped_without_gateway(monkeypatch, fake_email):
    """SMS is not sent when sms_carrier_gateway is None."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number="+15550001234",
            sms_notifications=True,
            sms_carrier_gateway=None,
        ),
    )

    send_notification(1, "Hello!")

    assert fake_email.calls == []


# ---------------------------------------------------------------------------
# Combined and edge cases
# ---------------------------------------------------------------------------


def test_sends_both_when_both_enabled(monkeypatch, fake_email):
    """Both email and SMS are sent when both channels are enabled."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=True,
            phone_number="+15550001234",
            sms_notifications=True,
            sms_carrier_gateway="tmomail.net",
        ),
    )

    send_notification(1, "Alert!")

    assert len(fake_email.calls) == 2


def test_sends_nothing_when_all_disabled(monkeypatch, fake_email):
    """No channel is contacted when all notifications are disabled."""
    _patch_prefs(
        monkeypatch,
        NotificationPreferences(
            email="u@example.com",
            email_notifications=False,
            phone_number=None,
            sms_notifications=False,
            sms_carrier_gateway=None,
        ),
    )

    send_notification(1, "Quiet!")

    assert fake_email.calls == []


def test_raises_for_unknown_user(monkeypatch, fake_email):
    """A ValueError is raised when the user does not exist in the database."""
    monkeypatch.setattr(
        dispatcher_module,
        "_load_preferences",
        lambda user_id: (_ for _ in ()).throw(ValueError(f"No user found with id={user_id}")),
    )

    with pytest.raises(ValueError, match="No user found"):
        send_notification(999, "Hello!")
