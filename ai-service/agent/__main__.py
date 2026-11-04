"""Draw the analyzer workflow: `uv run python -m agent [output.png]` (run from ai-service/)."""

import sys

from agent.analyze import save_graph_image

print(f"Graph written to {save_graph_image(*sys.argv[1:2]).resolve()}")
