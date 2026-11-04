"""Text formatting shared by the agents' prompts and tool results."""


def money(amount: float | None) -> str:
    """Return a dollar amount like `$1,234.50`, or `unknown` for None."""
    if amount is None:
        return "unknown"
    sign = "-" if amount < 0 else ""
    return f"{sign}${abs(amount):,.2f}"


def section(title: str, lines: list[str]) -> str:
    """Format a titled section of bullet lines, or say `- none` when there are no lines."""
    return f"{title}:\n" + ("\n".join(lines) if lines else "- none")
