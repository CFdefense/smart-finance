"""Small helpers shared by the tools."""

MAX_TOOL_CHARS = 4000


def cap(text: str) -> str:
    """Cut tool output to MAX_TOOL_CHARS so one tool cannot flood the prompt."""
    if len(text) <= MAX_TOOL_CHARS:
        return text
    return text[:MAX_TOOL_CHARS] + "\n... (truncated)"
