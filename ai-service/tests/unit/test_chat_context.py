"""Tests for the chat agent's financial context."""

from datetime import date

from langchain_core.messages import AIMessage

import agent.chat as chat_module
from agent import MAX_TRANSACTIONS, ChatAgent, ChatRequest, format_financial_context
from db import Account, Budget, Goal, Transaction

TODAY = date(2026, 11, 4)


def test_context_includes_every_section():
    """Accounts, budgets, goals and transactions all show up with formatted amounts."""
    text = format_financial_context(
        today=TODAY,
        accounts=[
            Account(
                account_id="a1",
                name="Checking",
                account_type="depository",
                account_subtype="checking",
                current_balance=1234.5,
                available_balance=None,
            )
        ],
        budgets=[
            Budget(
                name="Eating out", category="FOOD_DINING", monthly_limit=400, spent_this_month=90
            )
        ],
        goals=[
            Goal(
                name="Trip",
                account_name="Checking",
                target_amount=3000,
                baseline_balance=1000,
                current_balance=1234.5,
                target_date=date(2027, 6, 1),
            )
        ],
        transactions=[
            Transaction(
                transaction_id="t1", name="Refund", amount=-20, date=TODAY, category="SHOPPING"
            )
        ],
    )

    assert "Today is 2026-11-04." in text
    assert "Checking (checking): current $1,234.50, available unknown" in text
    assert "Budgets for November 2026" in text
    assert "Eating out [FOOD_DINING]: spent $90.00 of $400.00" in text
    assert "target $3,000.00 by 2027-06-01" in text
    assert "2026-11-04 Refund: -$20.00 [SHOPPING]" in text


def test_context_marks_empty_sections_and_truncation():
    """Empty sections say 'none', and a full transaction list says it was cut off."""
    txns = [
        Transaction(transaction_id=str(i), name="x", amount=1, date=TODAY)
        for i in range(MAX_TRANSACTIONS)
    ]

    text = format_financial_context(TODAY, accounts=[], budgets=[], goals=[], transactions=txns)

    assert "Accounts:\n- none" in text
    assert f"newest {MAX_TRANSACTIONS} only" in text


class FakeLLM:
    """Records the messages it gets and returns a fixed answer."""

    def invoke(self, messages):
        """Remember the messages and answer."""
        self.messages = messages
        return AIMessage(content="fake answer")


def test_chat_agent_puts_the_requesting_users_data_in_the_prompt(monkeypatch):
    """The agent loads context for the request's user and sends it in the system message."""
    loaded_for = []

    def loader(user_id: int) -> str:
        loaded_for.append(user_id)
        return "USER DATA"

    # Patch the name where chat.py looks it up, so no database is needed.
    monkeypatch.setattr(chat_module, "load_financial_context", loader)
    agent = ChatAgent("sk-test", "gpt-test")
    agent._llm = FakeLLM()

    response = agent.generate_answer(ChatRequest(user_id=7, message="How am I doing?"))

    assert response.response == "fake answer"
    assert loaded_for == [7]
    system, human = agent._llm.messages
    assert system.content.endswith("USER DATA")
    assert human.content == "How am I doing?"
