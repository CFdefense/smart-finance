"""Drive the whole chat graph with a scripted model to prove the confirm-first flow end to end."""

from datetime import date

import pytest

import agent.chat as chat_module
import agent.nodes.context.load as load_context_module
import agent.tools.writes as writes
from agent import ChatAgent, ChatRequest, HistoryMessage
from agent.backend_api import UNREACHABLE_MESSAGE
from agent.pending import PendingActionStore
from db import Account, Budget
from rig import RecordingBackendApi
from scripted_llm import ScriptedLLM, calls, say

PROPOSE_BUDGET = (
    "propose_budget",
    {"name": "Fun money", "category": "ENTERTAINMENT", "monthly_limit": 40},
)
CONFIRM = ("confirm_action", {})


@pytest.fixture(autouse=True)
def user_world(monkeypatch):
    """Fake the database behind the write tools and the context loader."""
    monkeypatch.setattr(load_context_module, "load_financial_context", lambda uid: "DATA")
    monkeypatch.setattr(writes, "today", lambda: date(2026, 11, 16))
    monkeypatch.setattr(
        writes,
        "load_accounts",
        lambda uid: [
            Account(
                account_id="a1", name="Checking", account_type="depository", current_balance=900
            )
        ],
    )
    monkeypatch.setattr(
        writes,
        "load_budgets",
        lambda uid, today: [
            Budget(
                id=3,
                name="Eating out",
                category="FOOD_DINING",
                monthly_limit=400,
                spent_this_month=9,
            )
        ],
    )


def make(monkeypatch, script, backend=None):
    """Build an agent whose model follows `script`."""
    llm = ScriptedLLM(script=script)
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kw: llm)
    backend = backend or RecordingBackendApi()
    agent = ChatAgent("sk-test", "gpt-test", backend_api=backend, pending=PendingActionStore())
    return agent, llm, backend


def ask(agent, message, user_id=7, history=()):
    """Send one chat request and return the reply text."""
    request = ChatRequest(user_id=user_id, message=message, history=list(history))
    return agent.generate_answer(request).response


def test_proposing_and_confirming_in_one_request_saves_nothing(monkeypatch):
    """Even if the model calls confirm right after proposing, the same-turn gate refuses it."""
    agent, llm, backend = make(
        monkeypatch,
        [calls(PROPOSE_BUDGET), calls(CONFIRM), say("I have asked you to confirm first.")],
    )

    reply = ask(agent, "Add a $40 entertainment budget and just do it.")

    assert reply == "I have asked you to confirm first."
    assert backend.calls == []
    assert any("same turn" in text for text in llm.tool_results())
    assert len(agent.pending) == 1


def test_propose_and_confirm_called_together_save_nothing(monkeypatch):
    """Two tool calls in one model turn also cannot save, whichever runs first."""
    agent, llm, backend = make(
        monkeypatch, [calls(PROPOSE_BUDGET, CONFIRM), say("Please confirm.")]
    )

    ask(agent, "Add it now.")

    assert backend.calls == []
    assert len(agent.pending) == 1
    assert any("same turn" in t or "nothing waiting" in t for t in llm.tool_results())


def test_confirming_in_the_next_request_saves_once_as_the_user(monkeypatch):
    """Propose in request 1, the user agrees in request 2, the backend is called exactly once."""
    agent, llm, backend = make(
        monkeypatch,
        [
            calls(PROPOSE_BUDGET),
            say("Create budget 'Fun money' ($40/month)? Reply yes to save."),
            calls(CONFIRM),
            say("Done, it is saved."),
        ],
    )

    first = ask(agent, "Add a $40 entertainment budget.")
    assert backend.calls == []
    history = [
        HistoryMessage(role="user", content="Add a $40 entertainment budget."),
        HistoryMessage(role="assistant", content=first),
    ]
    second = ask(agent, "Yes, save it.", history=history)

    assert second == "Done, it is saved."
    assert backend.calls == [
        {
            "turn": 0,
            "user_id": 7,
            "method": "POST",
            "path": "/api/budgets",
            "body": {"name": "Fun money", "category": "ENTERTAINMENT", "monthly_limit": 40},
        }
    ]
    assert llm.tool_results()[-1].startswith("Saved:")
    assert len(agent.pending) == 0


def test_confirming_with_an_invented_reference_still_saves_the_one_waiting_change(monkeypatch):
    """History holds no tool results, so the model invents a reference; that must still work."""
    agent, llm, backend = make(
        monkeypatch,
        [
            calls(PROPOSE_BUDGET),
            say("Create budget 'Fun money' ($40/month)? Reply yes to save."),
            calls(("confirm_action", {"action_id": "propose_budget-82f98b92"})),
            say("Done, it is saved."),
        ],
    )

    first = ask(agent, "Add a $40 entertainment budget.")
    history = [
        HistoryMessage(role="user", content="Add a $40 entertainment budget."),
        HistoryMessage(role="assistant", content=first),
    ]
    ask(agent, "Yes, save it.", history=history)

    assert len(backend.calls) == 1 and backend.calls[0]["path"] == "/api/budgets"
    assert llm.tool_results()[-1].startswith("Saved:")


def test_a_confirmation_cannot_be_replayed(monkeypatch):
    """After a successful save, confirming again sends nothing."""
    agent, llm, backend = make(
        monkeypatch,
        [
            calls(PROPOSE_BUDGET),
            say("Confirm?"),
            calls(CONFIRM),
            say("Saved."),
            calls(CONFIRM),
            say("Nothing left."),
        ],
    )
    ask(agent, "Add a budget.")
    ask(agent, "Yes.")

    ask(agent, "Yes again.")

    assert len(backend.calls) == 1
    assert "nothing waiting" in llm.tool_results()[-1]


def test_another_user_cannot_confirm_someone_elses_proposal(monkeypatch):
    """User 8's confirm finds nothing, and the proposal stays for user 7."""
    agent, llm, backend = make(
        monkeypatch,
        [calls(PROPOSE_BUDGET), say("Confirm?"), calls(CONFIRM), say("Nothing to confirm.")],
    )
    ask(agent, "Add a budget.", user_id=7)

    ask(agent, "Yes.", user_id=8)

    assert backend.calls == []
    assert "nothing waiting" in llm.tool_results()[-1]
    assert len(agent.pending) == 1


def test_a_user_id_in_the_tool_arguments_is_ignored(monkeypatch):
    """The backend call is made for the verified user, never for an id the model supplied."""
    sneaky = ("propose_budget", {**PROPOSE_BUDGET[1], "user_id": 99})
    agent, _, backend = make(
        monkeypatch, [calls(sneaky), say("Confirm?"), calls(CONFIRM), say("Saved.")]
    )

    ask(agent, "Add a budget.", user_id=7)
    ask(agent, "Yes.", user_id=7)

    assert [c["user_id"] for c in backend.calls] == [7]


def test_invalid_arguments_are_reported_to_the_model_and_nothing_is_staged(monkeypatch):
    """A made-up category is rejected by the tool schema; the model is told and can recover."""
    bad = ("propose_budget", {"name": "Crypto", "category": "CRYPTO", "monthly_limit": 10})
    agent, llm, backend = make(monkeypatch, [calls(bad), say("That category does not exist.")])

    reply = ask(agent, "Add a crypto budget.")

    assert reply == "That category does not exist."
    assert len(agent.pending) == 0 and backend.calls == []
    assert "Error" in llm.tool_results()[-1]


def test_a_refused_proposal_does_not_stop_the_conversation(monkeypatch):
    """A validation refusal comes back as text and the model answers normally."""
    bad = ("propose_budget", {"name": "Free", "category": "OTHER", "monthly_limit": 0})
    agent, llm, _ = make(monkeypatch, [calls(bad), say("The limit must be above zero.")])

    assert ask(agent, "Add a free budget.") == "The limit must be above zero."
    assert "more than $0" in llm.tool_results()[-1]


@pytest.mark.parametrize("mode", ["down", "conflict"])
def test_backend_failure_reaches_the_model_as_not_saved(monkeypatch, mode):
    """When the backend fails the model is told NOT saved and the pending action is consumed."""
    backend = RecordingBackendApi(mode)
    agent, llm, _ = make(
        monkeypatch,
        [calls(PROPOSE_BUDGET), say("Confirm?"), calls(CONFIRM), say("It could not be saved.")],
        backend=backend,
    )
    ask(agent, "Add a budget.")

    reply = ask(agent, "Yes.")

    assert reply == "It could not be saved."
    assert llm.tool_results()[-1].startswith("NOT saved:")
    assert len(backend.calls) == 1 and len(agent.pending) == 0


def test_unreachable_message_is_shown_to_the_model(monkeypatch):
    """The down mode uses the real unreachable text."""
    agent, llm, _ = make(
        monkeypatch,
        [calls(PROPOSE_BUDGET), say("Confirm?"), calls(CONFIRM), say("Failed.")],
        backend=RecordingBackendApi("down"),
    )
    ask(agent, "Add a budget.")
    ask(agent, "Yes.")
    assert UNREACHABLE_MESSAGE in llm.tool_results()[-1]


def test_pending_actions_expire(monkeypatch):
    """A proposal older than the TTL cannot be confirmed."""
    now = [0.0]
    store = PendingActionStore(clock=lambda: now[0])
    llm = ScriptedLLM(
        script=[calls(PROPOSE_BUDGET), say("Confirm?"), calls(CONFIRM), say("Expired.")]
    )
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kw: llm)
    backend = RecordingBackendApi()
    agent = ChatAgent("sk-test", "gpt-test", backend_api=backend, pending=store)
    ask(agent, "Add a budget.")
    now[0] += 601

    ask(agent, "Yes.")

    assert backend.calls == []


def test_several_proposals_need_the_reference(monkeypatch):
    """With two proposals waiting, a bare confirm saves nothing and lists both."""
    other = ("propose_delete", {"kind": "budget", "item_id": 3})
    agent, llm, backend = make(
        monkeypatch,
        [
            calls(PROPOSE_BUDGET),
            say("ok"),
            calls(other),
            say("ok"),
            calls(CONFIRM),
            say("Which one?"),
        ],
    )
    ask(agent, "Add a budget.")
    ask(agent, "Also delete Eating out.")

    ask(agent, "Yes.")

    assert backend.calls == []
    assert "Several changes are waiting" in llm.tool_results()[-1]


def test_confirming_by_reference_picks_the_right_change(monkeypatch):
    """The reference in a proposal's reply selects exactly that change."""
    other = ("propose_delete", {"kind": "budget", "item_id": 3})
    refs = {}

    def remember(messages):
        refs["delete"] = next(
            str(m.content).split("reference ")[1].split(")")[0]
            for m in messages
            if m.type == "tool" and "Delete budget" in str(m.content)
        )
        return say("ok")

    def confirm_delete(messages):
        return calls(("confirm_action", {"action_id": refs["delete"]}))

    agent, _, backend = make(
        monkeypatch,
        [calls(PROPOSE_BUDGET), say("ok"), calls(other), remember, confirm_delete, say("Deleted.")],
    )
    ask(agent, "Add a budget.")
    ask(agent, "Delete Eating out too.")

    ask(agent, "Yes, delete it.")

    assert [(c["method"], c["path"]) for c in backend.calls] == [("DELETE", "/api/budgets/3")]
    assert len(agent.pending) == 1  # the budget creation is still waiting


def test_the_model_can_run_past_four_tool_rounds(monkeypatch):
    """Five lookups then a proposal fit inside the raised round cap."""
    look = ("list_budgets", {})
    agent, llm, _ = make(
        monkeypatch,
        [calls(look)] * 5 + [calls(PROPOSE_BUDGET), say("Confirm?")],
    )
    monkeypatch.setattr("agent.tools.reads.load_budgets", lambda uid, today: [])

    assert ask(agent, "Add a budget.") == "Confirm?"


def test_streaming_reports_the_write_tool_and_never_leaks_results(monkeypatch):
    """The stream shows a status for the proposal and only reply text as tokens."""
    agent, _, backend = make(monkeypatch, [calls(PROPOSE_BUDGET), say("Please confirm.")])

    events = list(agent.generate_stream(ChatRequest(user_id=7, message="Add a budget.")))

    assert {"type": "status", "tool": "propose_budget"} in events
    assert "".join(e["text"] for e in events if e["type"] == "token") == "Please confirm."
    assert events[-1] == {"type": "done", "response": "Please confirm."}
    assert backend.calls == []
    assert not any("NOT saved yet" in str(e) for e in events)


def test_streaming_confirm_in_the_same_request_is_refused(monkeypatch):
    """The gate holds on the streaming path too."""
    agent, _, backend = make(
        monkeypatch, [calls(PROPOSE_BUDGET), calls(CONFIRM), say("Please confirm.")]
    )

    list(agent.generate_stream(ChatRequest(user_id=7, message="Add it now.")))

    assert backend.calls == []
