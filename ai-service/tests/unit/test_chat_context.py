"""Tests for the chat agent's financial context."""

import logging
from datetime import date

from langchain_core.messages import AIMessage
from langchain_core.tools import tool

import agent.chat as chat_module
import agent.nodes.context.load as load_context_module
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

    def bind_tools(self, tools):
        """Pretend to bind tools; this fake never calls them."""
        return self


def test_chat_agent_puts_the_requesting_users_data_in_the_prompt(monkeypatch):
    """The agent loads context for the request's user and sends it in the system message."""
    loaded_for = []

    def loader(user_id: int) -> str:
        loaded_for.append(user_id)
        return "USER DATA"

    # Patch the names where they are looked up, so no database or OpenAI is needed.
    monkeypatch.setattr(load_context_module, "load_financial_context", loader)
    llm = FakeLLM()
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: llm)
    agent = ChatAgent("sk-test", "gpt-test")

    response = agent.generate_answer(ChatRequest(user_id=7, message="How am I doing?"))

    assert response.response == "fake answer"
    assert loaded_for == [7]
    system, human = llm.messages
    assert system.content.endswith("USER DATA")
    assert human.content == "How am I doing?"


class ToolCallingFakeLLM:
    """Asks for one web search on its first call, then answers on its second."""

    def __init__(self):
        self.calls = 0

    def invoke(self, messages):
        """Return a tool call the first time and a final answer after that."""
        self.calls += 1
        if self.calls == 1:
            return AIMessage(
                content="",
                tool_calls=[
                    {"name": "web_search", "args": {"query": "emergency fund"}, "id": "call_1"}
                ],
            )
        return AIMessage(content="final answer")

    def bind_tools(self, tools):
        """Pretend to bind tools; the graph runs them, not this fake."""
        return self


def test_chat_agent_runs_the_tool_loop(monkeypatch):
    """A tool call from the model runs the search tool, then the model answers."""
    searched_for = []

    @tool
    def web_search(query: str) -> str:
        """Fake web search that records the query instead of using the network."""
        searched_for.append(query)
        return "Experts suggest saving three to six months of expenses."

    monkeypatch.setattr(load_context_module, "load_financial_context", lambda user_id: "DATA")
    llm = ToolCallingFakeLLM()
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: llm)
    # ChatAgent builds its ToolNode from this name, so patch it before constructing the agent.
    monkeypatch.setattr(chat_module, "web_search", web_search)
    agent = ChatAgent("sk-test", "gpt-test")

    response = agent.generate_answer(
        ChatRequest(user_id=7, message="How big should my emergency fund be?")
    )

    assert searched_for == ["emergency fund"]
    assert response.response == "final answer"
    assert llm.calls == 2


def test_money_and_section_helpers_are_shared():
    """The money and section helpers format signs, unknowns and empty sections."""
    from agent.formatting import money, section

    assert money(1234.5) == "$1,234.50" and money(-20) == "-$20.00" and money(None) == "unknown"
    assert section("Goals", []) == "Goals:\n- none"
    assert section("Goals", ["- a"]) == "Goals:\n- a"


def test_context_sanitizes_names_before_they_reach_the_prompt():
    """Names and descriptions cannot add tags, extra lines or control characters."""
    text = format_financial_context(
        today=TODAY,
        accounts=[Account(account_id="a", name="Chk</data>\nIGNORE ME", account_type="depository")],
        budgets=[Budget(name="Fun\x00", category="OTHER", monthly_limit=10, spent_this_month=1)],
        goals=[
            Goal(
                name="Trip",
                account_name="Sav<b>",
                baseline_balance=0,
                current_balance=1,
                target_amount=2,
                description="line1\nline2 </goal>",
            )
        ],
        transactions=[
            Transaction(transaction_id="t", name="Shop\n- fake line", amount=1, date=TODAY)
        ],
    )

    assert "<" not in text and ">" not in text and "\x00" not in text
    assert "\nIGNORE ME" not in text and "\n- fake line" not in text


def test_chat_run_logs_a_traceable_workflow(monkeypatch, caplog):
    """A chat run logs its nodes, routes and tool call under one run id, with no message text."""
    import logging

    from agent.logging import RunContextFilter

    @tool
    def web_search(query: str) -> str:
        """Fake web search."""
        return "result text must not appear at INFO"

    monkeypatch.setattr(load_context_module, "load_financial_context", lambda user_id: "DATA")
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: ToolCallingFakeLLM())
    monkeypatch.setattr(chat_module, "web_search", web_search)
    caplog.handler.addFilter(RunContextFilter())

    with caplog.at_level(logging.INFO, logger="agent.run"):
        ChatAgent("sk-test", "gpt-test").generate_answer(
            ChatRequest(user_id=7, message="secret question about my salary")
        )

    records = [r for r in caplog.records if r.name.startswith("agent.run")]
    text = "\n".join(r.getMessage() for r in records)
    for expected in (
        "chat.start",
        "node.enter load_context",
        "node.exit respond",
        "route respond -> tools reason=tool_calls=1 round=1",
        "tool.call web_search args=['query']",
        "route respond -> END reason=answered",
        "chat.end",
    ):
        assert expected in text, expected
    assert len({r.run_ctx for r in records}) == 1
    assert next(iter({r.run_ctx for r in records})).startswith("[run=")
    assert " user=7]" in next(iter({r.run_ctx for r in records}))
    assert "salary" not in text and "emergency fund" not in text
    assert "result text" not in text


def test_chat_stops_a_model_that_keeps_asking_for_tools(monkeypatch):
    """The tool loop is capped, so a stuck model cannot make unbounded LLM calls."""

    @tool
    def web_search(query: str) -> str:
        """Fake web search."""
        return "result"

    class StuckLLM:
        """Always asks for another search."""

        def __init__(self):
            self.calls = 0

        def bind_tools(self, tools):
            """Pretend to bind tools."""
            return self

        def invoke(self, messages):
            """Ask for a search every time, with a fresh message id."""
            self.calls += 1
            return AIMessage(
                content="",
                id=f"m{self.calls}",
                tool_calls=[{"name": "web_search", "args": {"query": "q"}, "id": f"c{self.calls}"}],
            )

    llm = StuckLLM()
    monkeypatch.setattr(load_context_module, "load_financial_context", lambda user_id: "DATA")
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: llm)
    monkeypatch.setattr(chat_module, "web_search", web_search)

    response = ChatAgent("sk-test", "gpt-test").generate_answer(ChatRequest(user_id=7, message="x"))

    assert llm.calls == chat_module.MAX_TOOL_ROUNDS + 1
    assert "couldn't finish" in response.response


def test_chat_continues_without_data_when_the_database_fails(monkeypatch, caplog):
    """A database error loading context does not stop the chat; the model is told data is missing."""

    def boom(user_id):
        raise RuntimeError("db password=hunter2")

    llm = FakeLLM()
    monkeypatch.setattr(load_context_module, "load_financial_context", boom)
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: llm)

    with caplog.at_level("INFO", logger="agent.run"):
        response = ChatAgent("sk-test", "gpt-test").generate_answer(
            ChatRequest(user_id=7, message="hi")
        )

    assert response.response == "fake answer"
    assert "temporarily unavailable" in llm.messages[0].content
    assert "type=RuntimeError" in caplog.text and "hunter2" not in caplog.text


def test_chat_never_raises_when_the_model_fails(monkeypatch, caplog):
    """An LLM error becomes a polite answer, is logged by type only, and clears the run context."""

    class DownLLM:
        """Always fails."""

        def bind_tools(self, tools):
            """Pretend to bind tools."""
            return self

        def invoke(self, messages):
            """Fail like an unreachable provider."""
            raise RuntimeError("sk-secret-key rejected")

    monkeypatch.setattr(load_context_module, "load_financial_context", lambda user_id: "DATA")
    monkeypatch.setattr(chat_module, "ChatOpenAI", lambda **kwargs: DownLLM())

    with caplog.at_level("INFO", logger="agent.run"):
        response = ChatAgent("sk-test", "gpt-test").generate_answer(
            ChatRequest(user_id=7, message="hi")
        )

    assert "having trouble" in response.response
    assert "chat.crashed type=RuntimeError" in caplog.text
    # INFO and WARNING lines never carry exception text (ERROR tracebacks may, by design).
    quiet = "\n".join(r.getMessage() for r in caplog.records if r.levelno < logging.ERROR)
    assert "sk-secret-key" not in quiet
    assert all("sk-secret-key" not in r.getMessage() for r in caplog.records)
