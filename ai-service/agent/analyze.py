from langchain_core.messages import HumanMessage, SystemMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from pydantic import BaseModel, PrivateAttr, SecretStr

from agent.data_models import AgentState, ChatRequest, ChatResponse


class AnalyzerAgent(BaseModel):
    """Agent responsible for automated transaction analysis."""

    _graph: CompiledStateGraph = PrivateAttr()

    def create(self, openai_key: str, model: str):
        """Create and compile the transaction analysis graph."""
        llm = ChatOpenAI(
            api_key=SecretStr(openai_key),
            model=model,
            temperature=0,
        )

        def analyze(state: AgentState) -> dict:
            """Evaluate a transaction-related request."""
            result = llm.invoke(
                [
                    SystemMessage(
                        content=(
                            "You are a financial transaction analysis agent. "
                            "Evaluate the provided transaction information and "
                            "explain whether it appears legitimate or suspicious. "
                            "Do not claim certainty without sufficient evidence."
                        )
                    ),
                    HumanMessage(content=state["message"]),
                ]
            )
            return {"response": str(result.content)}

        graph = StateGraph(AgentState)
        graph.add_node("analyze", analyze)
        graph.add_edge(START, "analyze")
        graph.add_edge("analyze", END)

        self._graph = graph.compile()
        return self

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
