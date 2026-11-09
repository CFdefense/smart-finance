"""
Run-scoped logging for the agent graphs: run ids, node tracing, tool and LLM callbacks.

Logging is always on and always verbose (DEBUG), and includes payment details and the content
nodes and tools read and write. Every line goes to `analyzer.log`; each analysis also gets its
own `log_<transaction_id>.txt`.
"""

import ast
import contextlib
import contextvars
import functools
import logging
import logging.handlers
import os
import re
import textwrap
import time
import uuid
from collections.abc import Callable
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from langchain_core.callbacks import BaseCallbackHandler
from pydantic import BaseModel

BASE = "agent.run"
GRAPH = f"{BASE}.graph"
NODE = f"{BASE}.node"
TOOL = f"{BASE}.tool"
LLM = f"{BASE}.llm"
DECISION = f"{BASE}.decision"

_HANDLER_MARKER = "_agent_run"

_run_id = contextvars.ContextVar("run_id", default="-")
_txn_id = contextvars.ContextVar("txn_id", default="-")
_user_id = contextvars.ContextVar("user_id", default="-")

_graph_log = logging.getLogger(GRAPH)
_node_log = logging.getLogger(NODE)
_tool_log = logging.getLogger(TOOL)
_llm_log = logging.getLogger(LLM)


def new_run_id() -> str:
    """Return a short id that ties together every log line of one agent run."""
    return uuid.uuid4().hex[:8]


def bind_run(run_id: str, transaction_id: str | None, user_id: int) -> None:
    """Attach the run, transaction (if any) and user ids to every log line in this context."""
    _run_id.set(run_id)
    _txn_id.set(transaction_id or "-")
    _user_id.set(str(user_id))


_TXN_TAIL = 4


class RunContextFilter(logging.Filter):
    """Add `run_ctx` (e.g. `[run=ab12 txn=9f3a user=7]`) and `component` (e.g. `node`)."""

    def filter(self, record: logging.LogRecord) -> bool:
        """Set the run context and component on the record and keep it."""
        if _run_id.get() == "-":
            record.run_ctx = "[-]"
        else:
            txn = f" txn={_txn_id.get()[-_TXN_TAIL:]}" if _txn_id.get() != "-" else ""
            record.run_ctx = f"[run={_run_id.get()}{txn} user={_user_id.get()}]"
        record.component = record.name.rsplit(".", 1)[-1]
        return True


_SECRET = re.compile(r"sk-[A-Za-z0-9_\-*.]{6,}|Bearer\s+[A-Za-z0-9._\-]{8,}")


class RedactSecretsFilter(logging.Filter):
    """Mask API keys and bearer tokens in messages and tracebacks (an OpenAI 401 echoes the key)."""

    def filter(self, record: logging.LogRecord) -> bool:
        """Rewrite the record with secrets replaced by `[redacted]` and keep it."""
        record.msg = _SECRET.sub("[redacted]", record.getMessage())
        record.args = None
        if record.exc_info and not record.exc_text:
            record.exc_text = logging.Formatter().formatException(record.exc_info)
        if record.exc_text:
            record.exc_text = _SECRET.sub("[redacted]", record.exc_text)
        return True


class _UtcIsoFormatter(logging.Formatter):
    """Format times like `2026-11-04T21:18:39.788Z` (UTC, milliseconds)."""

    def formatTime(  # noqa: N802 - overrides logging.Formatter
        self, record: logging.LogRecord, datefmt: str | None = None
    ) -> str:
        """Return the record's creation time as an ISO-8601 UTC timestamp."""
        stamp = datetime.fromtimestamp(record.created, tz=timezone.utc)
        return stamp.strftime("%Y-%m-%dT%H:%M:%S.") + f"{stamp.microsecond // 1000:03d}Z"


LOG_FILE = "analyzer.log"
_FORMAT = "[%(asctime)s] [PID:%(process)d] [%(levelname)s] [%(component)s] %(run_ctx)s %(message)s"


def default_log_dir() -> Path:
    """Return `$AGENT_LOG_DIR`, or `ai-service/logs` when it is not set."""
    return Path(os.getenv("AGENT_LOG_DIR") or Path(__file__).resolve().parent.parent / "logs")


def configure_agent_logging(log_dir: Path | None = None) -> None:
    """
    Send the agents' workflow logs, at DEBUG, to `<log_dir>/analyzer.log` (idempotent).

    Each line looks like `[2026-11-04T21:18:39.788Z] [PID:178] [INFO] [node] [run=... ] msg`.
    Nothing from the agent goes to stdout, so the terminal keeps only the request lines.
    """
    base = logging.getLogger(BASE)
    base.setLevel(logging.DEBUG)
    if any(getattr(h, _HANDLER_MARKER, False) for h in base.handlers):
        return
    directory = log_dir or default_log_dir()
    directory.mkdir(parents=True, exist_ok=True)
    handler = logging.handlers.RotatingFileHandler(
        directory / LOG_FILE, maxBytes=5_000_000, backupCount=3, encoding="utf-8", delay=True
    )
    setattr(handler, _HANDLER_MARKER, True)  # so repeated calls do not add handlers
    handler.addFilter(RedactSecretsFilter())
    handler.addFilter(RunContextFilter())
    handler.setFormatter(_UtcIsoFormatter(_FORMAT))
    base.addHandler(handler)
    base.propagate = False


_SAFE_FILE_ID = re.compile(r"[A-Za-z0-9_\-]{1,100}")


class _SafeFileHandler(logging.FileHandler):
    """A file handler that never raises, even when the lazy open fails (read-only dir, full disk)."""

    def emit(self, record: logging.LogRecord) -> None:
        """Write the record, reporting any failure through `handleError` instead of raising."""
        try:
            super().emit(record)
        except Exception:  # noqa: BLE001 - logging must never break an analysis
            self.handleError(record)


@contextlib.contextmanager
def run_log_file(transaction_id: str, log_dir: Path | None = None):
    """
    Also write this analysis's log lines to `<log_dir>/log_<transaction_id>.txt`.

    Call after `bind_run`. Only lines from the current run reach the file, so concurrent
    analyses never mix, even two runs of the same transaction (each writes its own lines once).
    Re-analysing the same transaction appends to its file. The transaction id is a validated
    identifier, so it is safe as a file name. A file that cannot be written never raises.
    """
    base = logging.getLogger(BASE)
    directory = log_dir or default_log_dir()
    if not _SAFE_FILE_ID.fullmatch(transaction_id):
        base.warning("run_log_file.skipped reason=unsafe_transaction_id")
        yield
        return
    try:
        directory.mkdir(parents=True, exist_ok=True)
    except OSError:
        base.warning("run_log_file.unavailable dir=%s", directory)
        yield
        return
    run_id = _run_id.get()
    handler = _SafeFileHandler(
        directory / f"log_{transaction_id}.txt", encoding="utf-8", delay=True
    )
    handler.addFilter(RedactSecretsFilter())
    handler.addFilter(RunContextFilter())
    handler.addFilter(lambda record: _run_id.get() == run_id)
    handler.setFormatter(_UtcIsoFormatter(_FORMAT))
    base.addHandler(handler)
    try:
        yield
    finally:
        base.removeHandler(handler)
        handler.close()


def traced_node(name: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """Wrap a graph node to log entry, exit (with timing and written keys) and failures."""

    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            _node_log.info("node.enter %s", name)
            started = time.perf_counter()
            try:
                result = func(*args, **kwargs)
            except Exception:
                _node_log.error("node.error %s", name, exc_info=True)
                raise
            ms = int((time.perf_counter() - started) * 1000)
            keys = sorted(result) if isinstance(result, dict) else []
            _node_log.info("node.exit %s ms=%d wrote=%s", name, ms, keys)
            if isinstance(result, dict):
                for key in keys:
                    _node_log.info(
                        "node.wrote %s.%s\n%s",
                        name,
                        key,
                        _box(f"{name}.{key}", _describe(result[key])),
                    )
            return result

        return wrapper

    return decorator


def log_route(source: str, target: str, reason: str) -> None:
    """Log which way a conditional edge went and why."""
    _graph_log.info("route %s -> %s reason=%s", source, target, reason)


_ARGS_MAX = 200


def _one_line(value: object, limit: int = _ARGS_MAX) -> str:
    """Render a value on one line, capped, so model-supplied text cannot forge log lines."""
    text = " ".join(str(value).split())
    return text if len(text) <= limit else text[:limit] + "..."


_BOX_WIDTH = 96
_BOX_MAX_LINES = 40


def _box(title: str, body: str) -> str:
    """
    Draw `body` in a box with `title` in the top edge, wrapped and capped.

    Every line starts with a box character, so content can never pass for a log line, and
    non-printable characters are replaced.
    """
    lines: list[str] = []
    for raw in body.splitlines() or [""]:
        clean = "".join(ch if ch.isprintable() else " " for ch in raw).rstrip()
        indent = " " * (len(clean) - len(clean.lstrip()))
        lines.extend(
            textwrap.wrap(
                clean,
                _BOX_WIDTH,
                subsequent_indent=indent + "  ",
                break_long_words=True,
                replace_whitespace=False,
            )
            or [""]
        )
    if len(lines) > _BOX_MAX_LINES:
        extra = len(lines) - _BOX_MAX_LINES
        lines = [*lines[:_BOX_MAX_LINES], f"... {extra} more line(s) not shown"]
    top = f"\u250c\u2500 {title} ".ljust(_BOX_WIDTH + 3, "\u2500") + "\u2510"
    bottom = "\u2514" + "\u2500" * (_BOX_WIDTH + 2) + "\u2518"
    middle = [f"\u2502 {line.ljust(_BOX_WIDTH)} \u2502" for line in lines]
    return "\n".join([top, *middle, bottom])


def _fields(data: dict[str, Any]) -> str:
    """Render a dict one field per line, with lists as bullet points."""
    out: list[str] = []
    for key, value in data.items():
        if isinstance(value, list):
            out.append(f"{key}:" if value else f"{key}: (none)")
            out.extend(f"  - {item}" for item in value)
        else:
            out.append(f"{key}: {value}")
    return "\n".join(out)


def _describe(value: object) -> str:
    """Render a value a node wrote for a box: models by field, messages by their last one."""
    if isinstance(value, BaseModel):
        return _fields(value.model_dump())
    if isinstance(value, list) and value and hasattr(value[-1], "type"):
        last = value[-1]
        calls = getattr(last, "tool_calls", None)
        head = f"{len(value)} message(s); last is from the {last.type}"
        if calls:
            return (
                head + "\ntool calls:\n" + "\n".join(f"  - {c['name']}({c['args']})" for c in calls)
            )
        return f"{head}\n{getattr(last, 'content', last)}"
    return str(value)


def _arg_names(input_str: object) -> list[str]:
    """Return the names of the arguments the model supplied, never their (untrusted) values."""
    try:
        parsed = ast.literal_eval(str(input_str))
    except (ValueError, SyntaxError):
        return []
    return sorted(str(key) for key in parsed) if isinstance(parsed, dict) else []


class AgentCallbackHandler(BaseCallbackHandler):
    """Log tool calls and LLM calls, including what the tools were given and returned."""

    def __init__(self) -> None:
        """Start with no in-flight timers."""
        self._started: dict[object, float] = {}
        self._names: dict[object, str] = {}
        self._inputs: dict[object, str] = {}

    def _elapsed_ms(self, run_id) -> int:
        """Return milliseconds since the call with this run id started, and forget the timer."""
        now = time.perf_counter()
        return int((now - self._started.pop(run_id, now)) * 1000)

    def on_tool_start(self, serialized, input_str, *, run_id=None, **kwargs):
        """
        Log which tool the model called and the arguments the model supplied.

        `input_str` holds only the model's arguments. The `inputs` kwarg is not used because it
        can include injected graph state (the user's transaction).
        """
        self._started[run_id] = time.perf_counter()
        name = (serialized or {}).get("name")
        self._names[run_id] = str(name)
        _tool_log.info("tool.call %s args=%s", name, _arg_names(input_str))
        self._inputs[run_id] = str(input_str)
        _tool_log.debug("tool.call.args %s %s", name, _one_line(input_str))

    def on_tool_end(self, output, *, run_id=None, **kwargs):
        """Log how big the result was, then the input and output in a box."""
        ms = self._elapsed_ms(run_id)
        text = str(getattr(output, "content", output))
        _tool_log.info("tool.result chars=%d ms=%d", len(text), ms)
        name = self._names.pop(run_id, "?")
        given = self._inputs.pop(run_id, "")
        _tool_log.info("tool.content %s\n%s", name, _box(name, f"input: {given}\n{text}"))
        _tool_log.debug("tool.result.content %s", _one_line(text, 20_000))

    def on_tool_error(self, error, *, run_id=None, **kwargs):
        """Log a failing tool call; the message (which may echo user text) only at DEBUG."""
        ms = self._elapsed_ms(run_id)
        self._names.pop(run_id, None)
        self._inputs.pop(run_id, None)
        _tool_log.warning("tool.error type=%s ms=%d", type(error).__name__, ms)
        _tool_log.debug("tool.error.message %s", _one_line(error, 2000))

    def on_chat_model_start(self, serialized, messages, *, run_id=None, **kwargs):
        """Log that the model is being called; prompts only at DEBUG."""
        self._started[run_id] = time.perf_counter()
        _llm_log.info("llm.start messages=%d", sum(len(m) for m in messages))
        _llm_log.debug("llm.prompt %s", messages)

    def on_llm_end(self, response, *, run_id=None, **kwargs):
        """Log call duration and token usage when the provider reports it."""
        ms = self._elapsed_ms(run_id)
        usage = (getattr(response, "llm_output", None) or {}).get("token_usage", {})
        _llm_log.info("llm.end ms=%d tokens=%s", ms, usage or "n/a")

    def on_llm_error(self, error, *, run_id=None, **kwargs):
        """Log a failing model call; the message only at DEBUG."""
        ms = self._elapsed_ms(run_id)
        _llm_log.warning("llm.error type=%s ms=%d", type(error).__name__, ms)
        _llm_log.debug("llm.error.message %s", _one_line(error, 2000))


def clear_run() -> None:
    """Remove the run context so later log lines are not tagged with a finished run."""
    _run_id.set("-")
    _txn_id.set("-")
    _user_id.set("-")
