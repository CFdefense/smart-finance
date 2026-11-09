"""Tests for the analyzer's run-scoped logging."""

import logging
import re

import pytest

from agent.logging import (
    GRAPH,
    NODE,
    AgentCallbackHandler,
    RunContextFilter,
    bind_run,
    configure_agent_logging,
    log_route,
    new_run_id,
    run_log_file,
    traced_node,
)


def test_new_run_id_is_short_and_unique():
    """Run ids are 8 hex characters and differ between runs."""
    first, second = new_run_id(), new_run_id()

    assert len(first) == 8 and int(first, 16) >= 0
    assert first != second


def test_filter_adds_run_context():
    """After bind_run, records carry the run, transaction and user ids."""
    bind_run("abcd1234", "txn-0123456789", 7)
    record = logging.LogRecord("x", logging.INFO, "f", 1, "m", None, None)

    assert RunContextFilter().filter(record) is True
    assert record.run_ctx == "[run=abcd1234 txn=6789 user=7]"


def test_traced_node_logs_enter_exit_and_written_keys(caplog):
    """A traced node logs entry, then exit with timing, the keys it wrote and their content."""

    @traced_node("load_context")
    def node(state):
        return {"context": "Checking $100", "status": "ok"}

    with caplog.at_level(logging.INFO, logger=NODE):
        node({"user_id": 1})

    messages = [r.getMessage() for r in caplog.records if r.name == NODE]
    assert messages[0] == "node.enter load_context"
    assert messages[1].startswith("node.exit load_context ms=")
    assert "wrote=['context', 'status']" in messages[1]
    assert "Checking $100" in " ".join(messages)


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
    with caplog.at_level(logging.INFO, logger="agent.run.graph"):
        log_route("claim", "END", "duplicate")

    assert "route claim -> END reason=duplicate" in caplog.text


def test_callback_handler_logs_tool_call_and_result(caplog):
    """Tool start logs the name and args; end logs the size and the input and output."""
    handler = AgentCallbackHandler()

    with caplog.at_level(logging.INFO, logger="agent.run.tool"):
        handler.on_tool_start({"name": "get_rules"}, "{'days': 3}", run_id=None)
        handler.on_tool_end("a rule description", run_id=None)

    assert "tool.call get_rules args=['days']" in caplog.text
    assert "input: {'days': 3}" in caplog.text
    assert "tool.result chars=" in caplog.text
    assert "a rule description" in caplog.text


def test_callback_handler_times_concurrent_tool_calls_independently(monkeypatch):
    """Two overlapping tool calls each get their own timer, keyed by run id."""
    handler = AgentCallbackHandler()
    clock = iter([0.0, 1.0, 3.0, 6.0])
    monkeypatch.setattr("agent.logging.time.perf_counter", lambda: next(clock))

    handler.on_tool_start({"name": "a"}, "{}", run_id="r1")  # t=0
    handler.on_tool_start({"name": "b"}, "{}", run_id="r2")  # t=1

    assert handler._elapsed_ms("r1") == 3000  # ended at t=3, started t=0
    assert handler._elapsed_ms("r2") == 5000  # ended at t=6, started t=1
    assert handler._started == {}


def test_callback_handler_error_messages_only_at_debug_and_single_line(caplog):
    """Error messages (which can echo user text) are not logged at INFO; args stay one line."""
    handler = AgentCallbackHandler()

    with caplog.at_level(logging.INFO, logger="agent.run"):
        handler.on_tool_start({"name": "t"}, "{'category': 'x\\nFAKE log line'}", run_id="r1")
        handler.on_tool_error(ValueError("input_value='secret merchant'"), run_id="r1")
        handler.on_llm_error(RuntimeError("echoed user text"), run_id="r2")

    assert "tool.call t args=['category']" in caplog.text
    assert "FAKE log line" not in caplog.text
    assert "tool.error type=ValueError" in caplog.text
    assert "llm.error type=RuntimeError" in caplog.text
    assert "secret merchant" not in caplog.text
    assert "echoed user text" not in caplog.text
    assert handler._started == {}


def test_configure_writes_workflow_lines_to_file_not_stdout(tmp_path, capsys):
    """Workflow lines go to analyzer.log in the dated format and nothing reaches stdout."""
    base = logging.getLogger("agent.run")
    saved, saved_level = list(base.handlers), base.level
    for handler in saved:
        base.removeHandler(handler)
    try:
        configure_agent_logging(tmp_path)
        assert base.level == logging.DEBUG  # always verbose
        bind_run("abcd1234", "txn-0123456789", 7)
        logging.getLogger(GRAPH).info("analyze.start amount=1.00")
        for handler in base.handlers:
            handler.flush()
    finally:
        for handler in list(base.handlers):
            base.removeHandler(handler)
            handler.close()
        for handler in saved:
            base.addHandler(handler)
        base.setLevel(saved_level)

    line = (tmp_path / "analyzer.log").read_text().strip()
    assert re.fullmatch(
        r"\[\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\] \[PID:\d+\] \[INFO\] \[graph\] "
        r"\[run=abcd1234 txn=6789 user=7\] analyze\.start amount=1\.00",
        line,
    )
    assert capsys.readouterr().out == ""


def test_run_log_file_keeps_each_transactions_lines_in_its_own_file(tmp_path):
    """Lines of a run go to log_<txid>.txt; a different transaction's lines stay out of it."""
    base = logging.getLogger("agent.run")
    saved_level = base.level
    base.setLevel(logging.DEBUG)
    try:
        bind_run("run1", "tx-A", 1)
        with run_log_file("tx-A", tmp_path):
            logging.getLogger(GRAPH).info("belongs to A")
            bind_run("run2", "tx-B", 2)  # another analysis interleaving on this thread
            logging.getLogger(GRAPH).info("belongs to B")
            bind_run("run1", "tx-A", 1)
            logging.getLogger(GRAPH).debug("debug for A")
        logging.getLogger(GRAPH).info("after the run")
    finally:
        base.setLevel(saved_level)

    text = (tmp_path / "log_tx-A.txt").read_text()
    assert "belongs to A" in text and "debug for A" in text
    assert "belongs to B" not in text and "after the run" not in text
    assert [p.name for p in tmp_path.iterdir()] == ["log_tx-A.txt"]


def test_run_log_file_writes_each_line_once_when_the_same_transaction_runs_twice(tmp_path):
    """Two overlapping runs of one transaction share the file but never duplicate lines."""
    base = logging.getLogger("agent.run")
    saved_level = base.level
    base.setLevel(logging.DEBUG)
    try:
        bind_run("run1", "tx-A", 1)
        with run_log_file("tx-A", tmp_path):
            bind_run("run2", "tx-A", 1)
            with run_log_file("tx-A", tmp_path):
                logging.getLogger(GRAPH).info("from run2")
            bind_run("run1", "tx-A", 1)
            logging.getLogger(GRAPH).info("from run1")
    finally:
        base.setLevel(saved_level)

    text = (tmp_path / "log_tx-A.txt").read_text()
    assert text.count("from run2") == 1 and text.count("from run1") == 1


def test_run_log_file_never_raises_when_the_file_cannot_be_written(tmp_path, monkeypatch):
    """A file that cannot be opened is reported through logging's error hook, not raised."""
    base = logging.getLogger("agent.run")
    saved_level = base.level
    base.setLevel(logging.DEBUG)
    monkeypatch.setattr(logging, "raiseExceptions", False)  # keep the report off stderr
    (tmp_path / "log_tx-A.txt").mkdir()  # opening a directory for append raises
    try:
        bind_run("run1", "tx-A", 1)
        with run_log_file("tx-A", tmp_path):
            logging.getLogger(GRAPH).info("still fine")
    finally:
        base.setLevel(saved_level)


def test_run_log_file_skips_an_unsafe_transaction_id(tmp_path):
    """An id that is not a plain file-name token never creates a file or escapes the folder."""
    bind_run("run1", "../evil", 1)
    with run_log_file("../evil", tmp_path):
        logging.getLogger(GRAPH).info("not written")

    assert list(tmp_path.iterdir()) == []
    assert not (tmp_path.parent / "log_../evil.txt").exists()


def test_tool_result_debug_dump_is_one_line(caplog):
    """Raw tool output at DEBUG cannot forge a log line with embedded newlines."""
    handler = AgentCallbackHandler()

    with caplog.at_level(logging.DEBUG, logger="agent.run.tool"):
        handler.on_tool_start({"name": "t"}, "{}", run_id="r1")
        handler.on_tool_end("ok\n[2026-01-01T00:00:00.000Z] [PID:1] [ERROR] fake", run_id="r1")

    dumps = [
        r.getMessage() for r in caplog.records if r.getMessage().startswith("tool.result.content")
    ]
    assert dumps and "\n" not in dumps[0]


def test_api_keys_are_redacted_from_messages_and_tracebacks(tmp_path):
    """An OpenAI 401 echoes the key; neither the message nor the traceback may reach the file."""
    key = "sk-proj-abcdEFGH1234"
    base = logging.getLogger("agent.run")
    saved, saved_level = list(base.handlers), base.level
    for handler in saved:
        base.removeHandler(handler)
    try:
        configure_agent_logging(tmp_path)
        try:
            raise RuntimeError(f"Incorrect API key provided: {key}. Authorization: Bearer {key}")
        except RuntimeError:
            logging.getLogger(NODE).error("node.error assess key=%s", key, exc_info=True)
        for handler in base.handlers:
            handler.flush()
    finally:
        for handler in list(base.handlers):
            base.removeHandler(handler)
            handler.close()
        for handler in saved:
            base.addHandler(handler)
        base.setLevel(saved_level)

    text = (tmp_path / "analyzer.log").read_text()
    assert "abcdEFGH1234" not in text
    assert text.count("[redacted]") >= 3
    assert "Traceback" in text  # the traceback is still there, just without the key


def test_content_logging_shows_what_nodes_and_tools_produced(caplog):
    """Node outputs and tool input/output are always logged in boxes."""
    caplog.set_level(logging.INFO)

    @traced_node("demo")
    def node(state):
        return {"context": "Accounts:\n- Checking $100", "notify": True}

    node({})
    handler = AgentCallbackHandler()
    handler.on_tool_start({"name": "get_rules"}, "{'days': 30}", run_id="r1")
    handler.on_tool_end("Rules:\n- Flag over $200", run_id="r1")
    handler.on_tool_start({"name": "get_rules"}, "{}", run_id="r2")
    handler.on_tool_error(ValueError("boom"), run_id="r2")

    assert "\u250c\u2500 demo.context " in caplog.text
    assert "\u2502 - Checking $100 " in caplog.text
    assert "\u2502 True " in caplog.text
    assert "\u250c\u2500 get_rules " in caplog.text
    assert "\u2502 input: {'days': 30} " in caplog.text
    assert "\u2502 - Flag over $200 " in caplog.text
    assert handler._names == {} and handler._inputs == {}


def test_box_wraps_caps_and_cleans_content():
    """Long lines wrap, tall content is cut with a note, and control characters are removed."""
    from agent.logging import _BOX_MAX_LINES, _BOX_WIDTH, _box

    wrapped = _box("t", "word " * 60).splitlines()
    assert wrapped[0].startswith("\u250c\u2500 t ") and wrapped[-1].startswith("\u2514")
    assert {len(line) for line in wrapped} == {_BOX_WIDTH + 4}

    tall = _box("t", "\n".join(f"line {i}" for i in range(_BOX_MAX_LINES + 10)))
    assert "10 more line(s) not shown" in tall

    forged = _box("t", "ok\x1b[31m\n2026-11-04 INFO fake line")
    assert "\x1b" not in forged
    assert all(line[0] in "\u250c\u2502\u2514" for line in forged.splitlines())


def test_describe_renders_models_by_field_and_message_lists_by_last_message():
    """Pydantic models list one field per line and message lists show their last message."""
    from langchain_core.messages import AIMessage, HumanMessage

    from agent.data_models import Assessment
    from agent.logging import _describe

    assessment = Assessment(
        verdict="OK",
        summary="fine",
        rule_violations=["too big"],
        should_notify=False,
        notify_reason="",
    )
    text = _describe(assessment)
    assert "verdict: OK" in text and "rule_violations:\n  - too big" in text
    assert "goal_notes: (none)" in text
    calls = [{"name": "get_rules", "args": {}, "id": "1"}]
    messages = [HumanMessage(content="hi"), AIMessage(content="", tool_calls=calls)]
    assert _describe(messages) == (
        "2 message(s); last is from the ai\ntool calls:\n  - get_rules({})"
    )
