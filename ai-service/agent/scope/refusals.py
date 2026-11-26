"""Fixed refusal texts. Nothing a model writes is ever used to refuse."""

# Do not reword these casually: `is_refusal` is an exact match against stored chat history, so
# turns saved with an older wording would stop being recognised and would leak back into context.
GENERAL_REFUSAL = (
    "I can only help with your personal finances in this app: your accounts, spending, "
    "budgets, rules and goals, and general money questions. Could you ask me something "
    "along those lines?"
)
ADVICE_REFUSAL = (
    "I can't give personalized investment or tax advice. I can explain how things like "
    "accounts, interest or taxes work in general, or look at your own spending, budgets "
    "and goals. What would you like to know?"
)
_REFUSALS = frozenset({GENERAL_REFUSAL, ADVICE_REFUSAL})


def refusal_for(category: str) -> str:
    """Return the fixed refusal for a refused category."""
    return ADVICE_REFUSAL if category == "personalized_advice" else GENERAL_REFUSAL


def is_refusal(text: str) -> bool:
    """True when `text` is exactly one of the fixed refusals."""
    return text in _REFUSALS
