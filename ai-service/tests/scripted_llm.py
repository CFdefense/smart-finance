"""A chat model that plays back a script, for tests that drive the whole graph."""

import uuid
from collections.abc import Callable
from typing import Any

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, BaseMessage
from langchain_core.outputs import ChatGeneration, ChatResult
from pydantic import Field


def say(text: str) -> AIMessage:
    """A final reply with no tool calls."""
    return AIMessage(content=text)


def calls(*pairs: tuple[str, dict[str, Any]]) -> AIMessage:
    """One model turn that asks for these tools (run together by the graph)."""
    return AIMessage(
        content="",
        tool_calls=[
            {"name": name, "args": args, "id": f"call_{uuid.uuid4().hex[:8]}"}
            for name, args in pairs
        ],
    )


class ScriptedLLM(BaseChatModel):
    """Replies from `script`, one item per model call; items may be callables of the messages."""

    script: list[Any]
    seen: list[list[BaseMessage]] = Field(default_factory=list)
    bound: list[str] = Field(default_factory=list)
    used: int = 0

    @property
    def _llm_type(self) -> str:
        return "scripted"

    def bind_tools(self, tools, **kwargs):
        """Remember the tool names; the graph runs the tools."""
        self.bound = [t.name for t in tools]
        return self

    def _generate(self, messages, stop=None, run_manager=None, **kwargs) -> ChatResult:
        self.seen.append(list(messages))
        if self.used >= len(self.script):
            raise AssertionError("the model was called more times than the script allows")
        item = self.script[self.used]
        self.used += 1
        if isinstance(item, Callable) and not isinstance(item, AIMessage):
            item = item(messages)
        message = AIMessage(content=item) if isinstance(item, str) else item
        return ChatResult(
            generations=[ChatGeneration(message=message.model_copy(update={"id": None}))]
        )

    def tool_results(self) -> list[str]:
        """Return the text of every tool result the model has been shown, oldest first."""
        last = self.seen[-1] if self.seen else []
        return [str(m.content) for m in last if m.type == "tool"]
