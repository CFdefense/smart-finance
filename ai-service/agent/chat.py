from langchain_core.messages import HumanMessage, SystemMessage
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph

from agent.context import load_financial_context
from agent.data_models import AgentState, ChatRequest, ChatResponse


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""

    def __init__(
        self,
        openai_key: str,
        model: str,
    ):
        """Create and compile the conversational graph."""
        self._llm = ChatOpenAI(
            api_key=openai_key,
            model=model,
        )

        graph = StateGraph(AgentState)
        graph.add_node("load_context", self.load_context)
        graph.add_node("respond", self.respond)
        graph.add_edge(START, "load_context")
        graph.add_edge("load_context", "respond")
        graph.add_edge("respond", END)

        self._graph = graph.compile()

    def load_context(self, state: AgentState) -> dict:
        """Look up the user's financial data and summarize it as text."""
        return {"context": load_financial_context(state["user_id"])}

    def respond(self, state: AgentState) -> dict:
        """Generate a conversational response grounded in the user's data."""
        result = self._llm.invoke(
            [
                SystemMessage(
                    content=(
                        "You are a helpful assistant for a budgeting app. "
                        "Answer using ONLY the user's data below. If the data doesn't "
                        "answer the question, say so instead of guessing.\n\n"
                        f"User's data:\n{state['context']}"
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
                "context": "",
                "response": "",
            }
        )
        return ChatResponse(response=result["response"])
