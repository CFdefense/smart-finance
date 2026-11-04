import logging
import time
from pathlib import Path

from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from langgraph.prebuilt import ToolNode
from pydantic import SecretStr

from agent.data_models import AnalysisResult, AnalyzeRequest, Assessment
from agent.logging import (
    GRAPH,
    AnalyzerCallbackHandler,
    bind_run,
    clear_run,
    new_run_id,
)
from agent.nodes.analysis import (
    MAX_TOOL_ROUNDS,
    claim_transaction,
    decide,
    finalize,
    load_account_context,
    make_assess,
    make_investigate,
    notify,
    route_after_assess,
    route_after_claim,
    route_after_decide,
    route_after_investigate,
)
from agent.state import AnalyzerState
from agent.tools import ALL_TOOLS

_log = logging.getLogger(GRAPH)

_LLM_TIMEOUT_SECONDS = 60

__all__ = ["MAX_TOOL_ROUNDS", "AnalyzerAgent", "build_analyzer_graph", "save_graph_image"]


def build_analyzer_graph(investigate_llm, assess_llm) -> CompiledStateGraph:
    """
    Wire the analyzer workflow.

    claim -> load_context -> investigate <-> tools -> assess -> decide -> notify? -> finalize
    """
    graph = StateGraph(AnalyzerState)
    graph.add_node("claim", claim_transaction)
    graph.add_node("load_context", load_account_context)
    graph.add_node("investigate", make_investigate(investigate_llm))
    graph.add_node("tools", ToolNode(ALL_TOOLS, handle_tool_errors=True))
    graph.add_node("assess", make_assess(assess_llm))
    graph.add_node("decide", decide)
    graph.add_node("notify", notify)
    graph.add_node("finalize", finalize)

    graph.add_edge(START, "claim")
    graph.add_conditional_edges(
        "claim", route_after_claim, {"load_context": "load_context", END: END}
    )
    graph.add_edge("load_context", "investigate")
    graph.add_conditional_edges(
        "investigate", route_after_investigate, {"tools": "tools", "assess": "assess"}
    )
    graph.add_edge("tools", "investigate")
    graph.add_conditional_edges(
        "assess", route_after_assess, {"decide": "decide", "finalize": "finalize"}
    )
    graph.add_conditional_edges(
        "decide", route_after_decide, {"notify": "notify", "finalize": "finalize"}
    )
    graph.add_edge("notify", "finalize")
    graph.add_edge("finalize", END)
    return graph.compile()


class AnalyzerAgent:
    """Agent that analyzes one transaction and notifies the user when it matters."""

    def __init__(self, openai_key: str, model: str):
        """Create the models and compile the analysis graph."""
        llm = ChatOpenAI(
            api_key=SecretStr(openai_key),
            model=model,
            temperature=0,
            timeout=_LLM_TIMEOUT_SECONDS,
            max_retries=1,
        )
        self._graph = build_analyzer_graph(
            llm.bind_tools(ALL_TOOLS), llm.with_structured_output(Assessment)
        )
        if _log.isEnabledFor(logging.DEBUG):
            _log.debug("graph.topology\n%s", self._graph.get_graph().draw_mermaid())

    def analyze(self, request: AnalyzeRequest) -> AnalysisResult:
        """Run the workflow for one transaction. Never raises; failures become a FAILED result."""
        txn = request.transaction
        run_id = new_run_id()
        bind_run(run_id, txn.transaction_id, request.user_id)
        try:
            started = time.perf_counter()
            _log.info(
                "analyze.start amount=%.2f category=%s pending=%s account=%s",
                txn.amount,
                txn.category,
                txn.pending,
                txn.account_id,
            )
            try:
                state = self._graph.invoke(
                    {
                        "run_id": run_id,
                        "user_id": request.user_id,
                        "transaction": txn,
                        "context": "",
                        "messages": [],
                        "assessment": None,
                        "notify": False,
                        "notified": False,
                        "status": "",
                        "error": None,
                    },
                    config={"callbacks": [AnalyzerCallbackHandler()], "run_name": "analyze"},
                )
                result = AnalysisResult(
                    transaction_id=txn.transaction_id,
                    status=state["status"],
                    assessment=state.get("assessment"),
                    notified=bool(state.get("notified")),
                    error=state.get("error"),
                )
            except Exception as exc:
                _log.error("analyze.crashed type=%s", type(exc).__name__)
                _log.debug("analyze.crashed detail", exc_info=True)
                result = AnalysisResult(
                    transaction_id=txn.transaction_id, status="FAILED", error="graph_error"
                )
            _log.info(
                "analyze.end status=%s verdict=%s notified=%s ms=%d",
                result.status,
                result.assessment.verdict if result.assessment else None,
                result.notified,
                int((time.perf_counter() - started) * 1000),
            )
            return result
        finally:
            clear_run()


def save_graph_image(path: str | Path = "analyzer_graph.png") -> Path:
    """
    Render the analyzer workflow as a PNG and return where it was written.

    No API key or database is needed: the graph is built without models. The image is drawn by
    the mermaid.ink web service, so this needs internet access.
    """
    graph = build_analyzer_graph(None, None)
    target = Path(path)
    target.write_bytes(graph.get_graph().draw_mermaid_png())
    return target
