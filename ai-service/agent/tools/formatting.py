"""Small helpers shared by the tools."""

MAX_TOOL_CHARS = 4000


def money(amount: float | None) -> str:
    """Return a dollar amount like `$1,234.50`, or `unknown` for None."""
    if amount is None:
        return "unknown"
    sign = "-" if amount < 0 else ""
    return f"{sign}${abs(amount):,.2f}"


def cap(text: str) -> str:
    """Cut tool output to MAX_TOOL_CHARS so one tool cannot flood the prompt."""
    if len(text) <= MAX_TOOL_CHARS:
        return text
    return text[:MAX_TOOL_CHARS] + "\n... (truncated)"
