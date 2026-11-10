"""promptfoo provider: prepare the persona's database, run the agent, return output + facts."""

import sys
from pathlib import Path

# promptfoo runs this file with test-framework/ on sys.path; the agent code lives one level up.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import evaldb  # noqa: E402
import facts  # noqa: E402
import runner  # noqa: E402
from definition import DefinitionError, parse  # noqa: E402


def call_api(prompt: str, options: dict, context: dict) -> dict:
    """Run one eval test. The prompt is only a display label; the test's vars drive the run."""
    test_vars = context["vars"]
    try:
        case = parse(test_vars)
        admin = evaldb.admin_url()
        persona_db = evaldb.ensure_persona(admin, test_vars["__evalId"], case.persona)
        accounts = evaldb.account_ids(persona_db.url, persona_db.user_id)
        evaldb.reset(persona_db.url, persona_db.user_id, case, accounts)
        output = runner.run(case, persona_db.url, persona_db.user_id, accounts)
        with runner.database(persona_db.url):
            snapshot = facts.snapshot(persona_db.user_id)
    except DefinitionError as exc:
        return {"error": f"invalid test definition: {exc}"}
    return {
        "output": output,
        "metadata": {"facts": snapshot, "persona": case.persona["email"], "agent": case.agent},
    }
