from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from pydantic import SecretStr

from agent.data_models import ChatRequest, ChatResponse
from agent.nodes import load_context, make_respond
from agent.state import AgentState


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""

    def __init__(self, openai_key: str, model: str):
        """Create and compile the conversational graph."""
        llm = ChatOpenAI(api_key=SecretStr(openai_key), model=model)

        graph = StateGraph(AgentState)
        graph.add_node("load_context", load_context)
        graph.add_node("respond", make_respond(llm))
        graph.add_edge(START, "load_context")
        graph.add_edge("load_context", "respond")
        graph.add_edge("respond", END)

        self._graph = graph.compile()

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Generate a conversational answer."""
        result = self._graph.invoke(
            {
                "user_id": request.user_id,
                "message": request.message,
                "context": "",
                "response": "",
            }
        )
        return ChatResponse(response=result["response"])
