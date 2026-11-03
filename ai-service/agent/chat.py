from langchain_core.messages import HumanMessage, SystemMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from pydantic import BaseModel, PrivateAttr, SecretStr

from agent.data_models import AgentState, ChatRequest, ChatResponse


class ChatAgent(BaseModel):
    """Agent responsible for handling user requests with financial context."""

    _graph: CompiledStateGraph = PrivateAttr()

    def create(self, openai_key: str, model: str):
        """Create and compile the conversational graph."""
        llm = ChatOpenAI(
            api_key=SecretStr(openai_key),
            model=model,
        )

        def respond(state: AgentState) -> dict:
            """Generate a conversational response."""
            result = llm.invoke(
                [
                    SystemMessage(
                        content=(
                            "You are a helpful assistant that answers questions "
                            "about financial information."
                        )
                    ),
                    HumanMessage(content=state["message"]),
                ]
            )
            return {"response": str(result.content)}

        graph = StateGraph(AgentState)
        graph.add_node("respond", respond)
        graph.add_edge(START, "respond")
        graph.add_edge("respond", END)

        self._graph = graph.compile()
        return self

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Generate a conversational answer."""
        result = self._graph.invoke(
            {
                "user_id": request.user_id,
                "message": request.message,
                "response": "",
            }
        )
        return ChatResponse(response=result["response"])
