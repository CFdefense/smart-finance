"""The labelled scope corpus: loading and validation."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from agent.data_models import HistoryMessage

CORPUS = Path(__file__).resolve().parent / "scope_corpus.yaml"
EXPECTATIONS = ("refuse", "allow")
SPLITS = ("dev", "holdout")


@dataclass(frozen=True)
class Case:
    """One labelled message."""

    id: str
    family: str
    expect: str
    split: str
    message: str
    history: tuple[HistoryMessage, ...] = ()
    core: bool = False


def load_corpus(path: Path = CORPUS) -> list[Case]:
    """Load and validate the corpus. Raises `ValueError` on a malformed or duplicate entry."""
    raw: list[dict[str, Any]] = yaml.safe_load(path.read_text(encoding="utf-8"))
    cases: list[Case] = []
    for item in raw:
        if item.get("expect") not in EXPECTATIONS or item.get("split") not in SPLITS:
            raise ValueError(f"{item.get('id')}: bad `expect` or `split`")
        cases.append(
            Case(
                id=item["id"],
                family=item["family"],
                expect=item["expect"],
                split=item["split"],
                message=item["message"],
                history=tuple(HistoryMessage(**turn) for turn in item.get("history", [])),
                core=bool(item.get("core", False)),
            )
        )
    ids = [c.id for c in cases]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate case ids in the scope corpus")
    return cases
