"""Tests for the analyzer's run-scoped logging."""

import logging

import pytest

from agent.logging import (
    NODE,
    AnalyzerCallbackHandler,
    RunContextFilter,
    bind_run,
    log_route,
    new_run_id,
    traced_node,
)


def test_new_run_id_is_short_and_unique():
    """Run ids are 8 hex characters and differ between runs."""
    first, second = new_run_id(), new_run_id()

    assert len(first) == 8 and int(first, 16) >= 0
    assert first != second


def test_filter_adds_run_context():
    """After bind_run, records carry the run, transaction and user ids."""
    bind_run("abcd1234", "txn-1", 7)
    record = logging.LogRecord("x", logging.INFO, "f", 1, "m", None, None)

    assert RunContextFilter().filter(record) is True
    assert record.run_ctx == "[run=abcd1234 txn=txn-1 user=7]"


def test_traced_node_logs_enter_exit_and_written_keys(caplog):
    """A traced node logs entry, then exit with timing and the state keys it wrote."""

    @traced_node("load_context")
    def node(state):
        return {"context": "secret text must not be logged", "status": "ok"}

    with caplog.at_level(logging.INFO, logger=NODE):
        node({"user_id": 1})

    messages = [r.getMessage() for r in caplog.records if r.name == NODE]
    assert messages[0] == "node.enter load_context"
    assert messages[1].startswith("node.exit load_context ms=")
    assert "wrote=['context', 'status']" in messages[1]
    assert "secret text" not in " ".join(messages)


def test_traced_node_logs_traceback_and_reraises(caplog):
    """A failing node logs an ERROR with the exception and re-raises it."""

    @traced_node("assess")
    def node(state):
        raise RuntimeError("boom")

    with caplog.at_level(logging.INFO, logger=NODE), pytest.raises(RuntimeError):
        node({})

    errors = [r for r in caplog.records if r.levelno == logging.ERROR]
    assert errors and errors[0].exc_info is not None
    assert "node.error assess" in errors[0].getMessage()


def test_log_route_records_target_and_reason(caplog):
    """Routing decisions are logged with the reason."""
    with caplog.at_level(logging.INFO, logger="agent.analyze.graph"):
        log_route("claim", "END", "duplicate")

    assert "route claim -> END reason=duplicate" in caplog.text


def test_callback_handler_logs_tool_call_and_result(caplog):
    """Tool start logs the name and args; end logs size, not content, at INFO."""
    handler = AnalyzerCallbackHandler()

    with caplog.at_level(logging.INFO, logger="agent.analyze.tool"):
        handler.on_tool_start({"name": "get_rules"}, "{'days': 3}", run_id=None)
        handler.on_tool_end("a very private rule description", run_id=None)

    assert "tool.call get_rules args={'days': 3}" in caplog.text
    assert "tool.result chars=" in caplog.text
    assert "private rule" not in caplog.text


def test_callback_handler_times_concurrent_tool_calls_independently(monkeypatch):
    """Two overlapping tool calls each get their own timer, keyed by run id."""
    handler = AnalyzerCallbackHandler()
    clock = iter([0.0, 1.0, 3.0, 6.0])
    monkeypatch.setattr("agent.logging.time.perf_counter", lambda: next(clock))

    handler.on_tool_start({"name": "a"}, "{}", run_id="r1")  # t=0
    handler.on_tool_start({"name": "b"}, "{}", run_id="r2")  # t=1

    assert handler._elapsed_ms("r1") == 3000  # ended at t=3, started t=0
    assert handler._elapsed_ms("r2") == 5000  # ended at t=6, started t=1
    assert handler._started == {}


def test_callback_handler_error_messages_only_at_debug_and_single_line(caplog):
    """Error messages (which can echo user text) are not logged at INFO; args stay one line."""
    handler = AnalyzerCallbackHandler()

    with caplog.at_level(logging.INFO, logger="agent.analyze"):
        handler.on_tool_start({"name": "t"}, "line1\nFAKE log line", run_id="r1")
        handler.on_tool_error(ValueError("input_value='secret merchant'"), run_id="r1")
        handler.on_llm_error(RuntimeError("echoed user text"), run_id="r2")

    assert "tool.call t args=line1 FAKE log line" in caplog.text
    assert "tool.error type=ValueError" in caplog.text
    assert "llm.error type=RuntimeError" in caplog.text
    assert "secret merchant" not in caplog.text
    assert "echoed user text" not in caplog.text
    assert handler._started == {}
