from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from pydantic import SecretStr

from agent.data_models import ChatRequest, ChatResponse
from agent.nodes import make_analyze
from agent.state import AgentState


class AnalyzerAgent:
    """Agent responsible for automated transaction analysis."""

    def __init__(self, openai_key: str, model: str):
        """Create and compile the transaction analysis graph."""
        llm = ChatOpenAI(api_key=SecretStr(openai_key), model=model, temperature=0)

        graph = StateGraph(AgentState)
        graph.add_node("analyze", make_analyze(llm))
        graph.add_edge(START, "analyze")
        graph.add_edge("analyze", END)

        self._graph = graph.compile()

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Run transaction analysis and return the result."""
        result = self._graph.invoke(
            {
                "user_id": request.user_id,
                "message": request.message,
                "context": "",
                "response": "",
            }
        )
        return ChatResponse(response=result["response"])
