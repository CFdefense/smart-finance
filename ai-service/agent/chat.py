from langchain_core.messages import HumanMessage, SystemMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph

from agent.data_models import AgentState, ChatRequest, ChatResponse


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""
    
    def __init__(self, openai_key: str, model: str):
        """Create and compile the conversational graph."""
        self._llm = ChatOpenAI(
            api_key=openai_key,
            model=model,
        )

        graph = StateGraph(AgentState)
        graph.add_node("respond", self.respond)
        graph.add_edge(START, "respond")
        graph.add_edge("respond", END)

        self._graph = graph.compile()

    def respond(self, state: AgentState) -> dict:
            """Generate a conversational response."""
            result = self._llm.invoke(
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
