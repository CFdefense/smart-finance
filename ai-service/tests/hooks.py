"""
promptfoo extension hook: keep the machine free of leftover eval databases.

Each hook call runs in its own Python process, so nothing is shared with the provider;
eval databases are found by their `smartfinance_eval_` name prefix instead.
"""

import evaldb


def on_event(hook_name: str, context: dict) -> None:
    """Sweep every eval database before a run, and drop this run's databases afterwards."""
    if hook_name == "beforeAll":
        evaldb.drop_databases(evaldb.admin_url())
    elif hook_name == "afterAll":
        evaldb.drop_databases(evaldb.admin_url(), context["evalId"])
