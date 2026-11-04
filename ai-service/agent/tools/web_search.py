import os

import httpx
from langchain_core.tools import tool


@tool
def web_search(query: str) -> str:
    """
    Search the web for general personal-finance information.

    Use it for things like tax rules, account types, or interest-rate definitions.
    Never include the user's personal data in the query.
    """
    resp = httpx.post(
        "https://api.tavily.com/search",
        headers={"Authorization": f"Bearer {os.environ['TAVILY_API_KEY']}"},
        json={"query": query, "max_results": 5},
        timeout=10,
    )
    resp.raise_for_status()
    return "\n\n".join(f"{r['title']} ({r['url']})\n{r['content']}" for r in resp.json()["results"])
