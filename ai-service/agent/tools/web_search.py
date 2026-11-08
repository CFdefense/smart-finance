"""Tool: search the web for general personal-finance information."""

import logging
import os

import httpx
from langchain_core.tools import tool

from agent.logging import TOOL
from agent.safety import fence
from agent.tools.formatting import cap

_log = logging.getLogger(TOOL)

_UNAVAILABLE = "Web search is unavailable right now. Answer without it and say so."
_MAX_RESULTS = 5
_TITLE_MAX = 100
_CONTENT_MAX = 600


@tool
def web_search(query: str) -> str:
    """
    Search the web for general personal-finance information.

    Use it for things like tax rules, account types, or interest-rate definitions.
    Never include the user's personal data in the query.
    """
    api_key = os.environ.get("TAVILY_API_KEY")
    if not api_key:
        _log.warning("web_search.unavailable reason=no_api_key")
        return _UNAVAILABLE
    try:
        resp = httpx.post(
            "https://api.tavily.com/search",
            headers={"Authorization": f"Bearer {api_key}"},
            json={"query": query, "max_results": _MAX_RESULTS},
            timeout=10,
        )
        resp.raise_for_status()
        results = resp.json()["results"]
    except (httpx.HTTPError, KeyError, ValueError) as exc:
        _log.warning("web_search.failed type=%s", type(exc).__name__)
        _log.debug("web_search.failed detail", exc_info=True)
        return _UNAVAILABLE
    if not results:
        return "No results."
    # Page text is untrusted: fence it so the prompt can mark it as data, never instructions.
    return cap(
        "\n\n".join(
            f"{fence('search_title', r.get('title', ''), _TITLE_MAX)} ({r.get('url', '')})\n"
            f"{fence('search_result', r.get('content', ''), _CONTENT_MAX)}"
            for r in results
        )
    )
