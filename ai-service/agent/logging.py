"""Run-scoped logging for the analyzer graph: run ids, node tracing, tool and LLM callbacks."""

import contextvars
import functools
import logging
import time
import uuid
from collections.abc import Callable

from langchain_core.callbacks import BaseCallbackHandler

GRAPH = "agent.analyze.graph"
NODE = "agent.analyze.node"
TOOL = "agent.analyze.tool"
LLM = "agent.analyze.llm"
DECISION = "agent.analyze.decision"

_run_id = contextvars.ContextVar("run_id", default="-")
_txn_id = contextvars.ContextVar("txn_id", default="-")
_user_id = contextvars.ContextVar("user_id", default="-")

_graph_log = logging.getLogger(GRAPH)
_node_log = logging.getLogger(NODE)
_tool_log = logging.getLogger(TOOL)
_llm_log = logging.getLogger(LLM)


def new_run_id() -> str:
    """Return a short id that ties together every log line of one analysis run."""
    return uuid.uuid4().hex[:8]


def bind_run(run_id: str, transaction_id: str, user_id: int) -> None:
    """Attach the run, transaction and user ids to every log line made in this context."""
    _run_id.set(run_id)
    _txn_id.set(transaction_id)
    _user_id.set(str(user_id))


class RunContextFilter(logging.Filter):
    """Add `run_ctx` (e.g. `[run=ab12 txn=t1 user=7]`) to every record."""

    def filter(self, record: logging.LogRecord) -> bool:
        """Set the run context on the record and keep it."""
        if _run_id.get() == "-":
            record.run_ctx = "[-]"
        else:
            record.run_ctx = f"[run={_run_id.get()} txn={_txn_id.get()} user={_user_id.get()}]"
        return True


def configure_agent_logging(level: str = "INFO") -> None:
    """Set the analyzer log level and make its lines include the run context (idempotent)."""
    base = logging.getLogger("agent.analyze")
    base.setLevel(level.upper())
    if any(getattr(h, "_agent_analyze", False) for h in base.handlers):
        return
    handler = logging.StreamHandler()
    handler._agent_analyze = True  # marker so repeated calls do not add handlers
    handler.addFilter(RunContextFilter())
    handler.setFormatter(
        logging.Formatter("%(asctime)s %(levelname)s %(name)s %(run_ctx)s %(message)s")
    )
    base.addHandler(handler)
    base.propagate = False


def traced_node(name: str) -> Callable:
    """Wrap a graph node to log entry, exit (with timing and written keys) and failures."""

    def decorator(func: Callable) -> Callable:
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


class AnalyzerCallbackHandler(BaseCallbackHandler):
    """Log tool calls and LLM calls. Content appears only at DEBUG."""

    def __init__(self) -> None:
        """Start with no in-flight timers."""
        self._started: dict[object, float] = {}

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
        _tool_log.info("tool.call %s args=%s", (serialized or {}).get("name"), _one_line(input_str))

    def on_tool_end(self, output, *, run_id=None, **kwargs):
        """Log how big the result was; the content itself only at DEBUG."""
        ms = self._elapsed_ms(run_id)
        text = str(getattr(output, "content", output))
        _tool_log.info("tool.result chars=%d ms=%d", len(text), ms)
        _tool_log.debug("tool.result.content %s", text)

    def on_tool_error(self, error, *, run_id=None, **kwargs):
        """Log a failing tool call; the message (which may echo user text) only at DEBUG."""
        ms = self._elapsed_ms(run_id)
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
