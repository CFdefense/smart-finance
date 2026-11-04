"""Draw an agent's workflow: `uv run python -m agent analyzer|chat [output.png]` (from ai-service/)."""

import sys

from agent.graph_image import GRAPHS, save_graph_image

usage = f"usage: python -m agent {'|'.join(GRAPHS)} [output.png]"
if len(sys.argv) < 2:
    sys.exit(usage)

# Accept any unique prefix, so `analyze` works for `analyzer`.
matches = [name for name in GRAPHS if name.startswith(sys.argv[1])]
if len(matches) != 1:
    sys.exit(f"unknown agent '{sys.argv[1]}'\n{usage}")

name = matches[0]
output = sys.argv[2] if len(sys.argv) > 2 else f"{name}_graph.png"
print(f"{name} graph written to {save_graph_image(GRAPHS[name](), output).resolve()}")
