"""Shared test fixtures."""

import logging

import pytest

from agent.logging import clear_run


@pytest.fixture(autouse=True)
def analyzer_logs_reach_caplog():
    """Let analyzer log records propagate to the root logger so `caplog` sees them."""
    logger = logging.getLogger("agent.analyze")
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
