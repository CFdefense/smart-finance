"""Helpers for handling untrusted text (transaction names, rule descriptions, model output)."""

import re
import unicodedata

NOTIFY_REASON_MAX = 200
_NAME_MAX = 60
_CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_ANGLES = re.compile(r"[<>]")
_SPACE = re.compile(r"\s+")


def sanitize(text: str, max_len: int) -> tuple[str, int]:
    """
    Make `text` safe to put in a prompt or message.

    Normalizes Unicode (NFKC), removes invisible/format and control characters and angle brackets, collapses whitespace and cuts to `max_len`.
    Returns the clean text and how many characters were removed or cut.
    """
    cleaned = unicodedata.normalize("NFKC", text)  # folds full-width < > to ASCII
    cleaned = "".join(c for c in cleaned if unicodedata.category(c) != "Cf")  # bidi, zero-width
    cleaned = _CONTROL.sub("", cleaned)
    cleaned = _ANGLES.sub("", cleaned)
    cleaned = _SPACE.sub(" ", cleaned).strip()
    cleaned = cleaned[:max_len]
    return cleaned, max(len(text) - len(cleaned), 0) if cleaned != text else 0


def fence(tag: str, text: str, max_len: int = 300) -> str:
    """Wrap sanitized `text` in `<tag>` markers so the prompt can mark it as data."""
    clean, _ = sanitize(text, max_len)
    return f"<{tag}>{clean}</{tag}>"


def build_notification(name: str, amount: float, reason: str) -> str:
    """Build the notification text from a fixed template and a sanitized model reason."""
    clean_name, _ = sanitize(name, _NAME_MAX)
    clean_reason, _ = sanitize(reason, NOTIFY_REASON_MAX)
    return f"{clean_name} ${amount:,.2f}: {clean_reason}"
