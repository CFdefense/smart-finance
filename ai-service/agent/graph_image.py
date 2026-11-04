"""Draw an agent's workflow graph as a PNG."""

from pathlib import Path

from langgraph.graph.state import CompiledStateGraph

from agent.analyze import build_analyzer_graph
from agent.chat import build_chat_graph

# Each graph is built without models: they are only called at run time.
GRAPHS = {
    "analyzer": lambda: build_analyzer_graph(None, None),
    "chat": lambda: build_chat_graph(None),
}


def save_graph_image(graph: CompiledStateGraph, path: str | Path) -> Path:
    """
    Render `graph` as a PNG at `path` and return the path.

    The image is drawn by the mermaid.ink web service, so this needs internet access.
    """
    target = Path(path)
    target.write_bytes(graph.get_graph().draw_mermaid_png())
    return target
