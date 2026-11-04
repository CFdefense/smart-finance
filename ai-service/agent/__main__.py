"""Draw an agent's workflow: `uv run python -m agent analyzer|chat [output.png]` (from ai-service/)."""

import sys

from agent.graph_image import GRAPHS, save_graph_image

if len(sys.argv) < 2 or sys.argv[1] not in GRAPHS:
    sys.exit(f"usage: python -m agent {'|'.join(GRAPHS)} [output.png]")

name = sys.argv[1]
output = sys.argv[2] if len(sys.argv) > 2 else f"{name}_graph.png"
print(f"{name} graph written to {save_graph_image(GRAPHS[name](), output).resolve()}")
