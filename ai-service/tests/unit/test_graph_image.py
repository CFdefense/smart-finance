"""Tests for drawing the agents' workflow graphs."""

import pytest

from agent import graph_image
from agent.graph_image import GRAPHS, save_graph_image


@pytest.mark.parametrize("name,expected", [("analyzer", "claim"), ("chat", "respond")])
def test_each_agent_graph_builds_without_models(name, expected):
    """Both graphs can be built with no LLM or key, and contain their own nodes."""
    nodes = GRAPHS[name]().get_graph().nodes

    assert expected in nodes


def test_save_graph_image_writes_the_rendered_png(tmp_path):
    """The PNG bytes from the graph renderer are written to the requested path."""

    class FakeDrawable:
        def draw_mermaid_png(self):
            return b"\x89PNG fake"

    class FakeGraph:
        def get_graph(self):
            return FakeDrawable()

    written = save_graph_image(FakeGraph(), tmp_path / "g.png")

    assert written == tmp_path / "g.png" and written.read_bytes() == b"\x89PNG fake"
    assert graph_image.GRAPHS.keys() == {"analyzer", "chat"}
