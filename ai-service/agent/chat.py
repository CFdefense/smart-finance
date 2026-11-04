from langchain_openai import ChatOpenAI
from langgraph.graph import START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from langgraph.prebuilt import ToolNode, tools_condition
from pydantic import SecretStr

from agent.data_models import ChatRequest, ChatResponse
from agent.nodes import load_context, make_respond
from agent.state import ChatState
from agent.tools import web_search


def build_chat_graph(llm_with_tools, tools) -> CompiledStateGraph:
    """Wire the chat workflow: load_context -> respond <-> tools."""
    graph = StateGraph(ChatState)
    graph.add_node("load_context", load_context)
    graph.add_node("respond", make_respond(llm_with_tools))
    graph.add_node("tools", ToolNode(tools, handle_tool_errors=True))
    graph.add_edge(START, "load_context")
    graph.add_edge("load_context", "respond")
    graph.add_conditional_edges("respond", tools_condition)
    graph.add_edge("tools", "respond")
    return graph.compile()


class ChatAgent:
    """Agent responsible for handling user requests with financial context."""

    def __init__(self, openai_key: str, model: str):
        """Create and compile the conversational graph."""
        tools = [web_search]
        llm = ChatOpenAI(api_key=SecretStr(openai_key), model=model)
        self._graph = build_chat_graph(llm.bind_tools(tools), tools)

    def generate_answer(self, request: ChatRequest) -> ChatResponse:
        """Generate a conversational answer."""
        result = self._graph.invoke(
            {
                "user_id": request.user_id,
                "context": "",
                "messages": [request.message],
            }
        )
        return ChatResponse(response=result["messages"][-1].text)
