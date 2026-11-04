"""Shared test fixtures."""

import logging

import pytest


@pytest.fixture(autouse=True)
def analyzer_logs_reach_caplog():
    """Let analyzer log records propagate to the root logger so `caplog` sees them."""
    logger = logging.getLogger("agent.analyze")
    previous = logger.propagate
    logger.propagate = True
    yield
    logger.propagate = previous
