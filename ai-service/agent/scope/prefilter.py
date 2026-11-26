"""A free, deterministic first check: obvious jailbreak phrases, empty and oversized messages."""

import re
import unicodedata

from agent.safety import sanitize
from agent.scope.guard import ScopeDecision

MAX_MESSAGE_CHARS = 2000

# Quoted spans are dropped before matching so a quoted merchant name is not read as an instruction.
_QUOTED = re.compile(
    r"\"[^\"]*\"|\u201c[^\u201d]*\u201d|\u2018[^\u2019]*\u2019|(?<!\w)'[^']*'(?!\w)"
)
_REVEAL = r"\b(?:print|reveal|repeat|show|display|output|tell)\b[^.?!]{0,25}"
_OVERRIDE = r"\b(?:ignore|disregard|forget)\s+"
_EARLIER = r"(?:previous|prior|above|earlier)\s+"
# "the previous rules" is ordinary finance talk (budget rules), so a bare "rules" needs "your".
_SIGNATURES = (
    re.compile(
        _OVERRIDE
        + r"(?:(?:all|any)\s+(?:of\s+)?(?:your\s+|the\s+)?|your\s+)"
        + _EARLIER
        + r"(?:instructions|prompts?)\b"
    ),
    re.compile(_OVERRIDE + r"(?:(?:all|any)\s+(?:of\s+)?)?your\s+" + _EARLIER + r"rules\b"),
    re.compile(r"\bdeveloper mode\b"),
    re.compile(r"\bdo anything now\b"),
    re.compile(_REVEAL + r"\b(?:your|the)\s+(?:system|initial|hidden)\s+(?:prompt|instructions)\b"),
)


def prefilter(message: str) -> ScopeDecision | None:
    """Return a refusal decision for an obvious violation, or None to let the classifier decide."""
    if len(message) > MAX_MESSAGE_CHARS:
        return ScopeDecision(category="off_topic", layer="prefilter")
    # Angle brackets become spaces first, so `<b>ignore ...</b>` keeps its word boundaries.
    text = unicodedata.normalize("NFKC", message).replace("<", " ").replace(">", " ")
    clean, _ = sanitize(text, MAX_MESSAGE_CHARS)
    if not clean:
        return ScopeDecision(category="off_topic", layer="prefilter")
    searchable = _QUOTED.sub(" ", clean.lower())
    if any(signature.search(searchable) for signature in _SIGNATURES):
        return ScopeDecision(category="manipulation", layer="prefilter")
    return None
