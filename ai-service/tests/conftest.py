"""Shared test fixtures."""

import logging
import os
import tempfile

import pytest

# Set before `main` is imported so tests never write to the real ai-service/logs folder.
os.environ["AGENT_LOG_DIR"] = tempfile.mkdtemp(prefix="agent-test-logs-")

from agent.logging import clear_run


@pytest.fixture(autouse=True)
def analyzer_logs_reach_caplog():
    """Let analyzer log records propagate to the root logger so `caplog` sees them."""
    logger = logging.getLogger("agent.run")
    previous = logger.propagate
    logger.propagate = True
    yield
    logger.propagate = previous


@pytest.fixture(autouse=True)
def clear_run_context():
    """Reset the run context so one test's ids never tag another test's log lines."""
    clear_run()
    yield
    clear_run()
