"""Tests for the classifier eval's expansion, gating and report, with a fake guard."""

from agent.scope import ScopeCheckError, ScopeDecision
from scope_cases import Case
from scope_eval import MIN_RATE, Result, evaluate, expand, gate, main, render
from scope_mutations import MUTATORS


def case(id, expect="refuse", split="dev", core=False, family="f", message="m"):
    """Build a corpus case."""
    return Case(id=id, family=family, expect=expect, split=split, message=message, core=core)


class FakeGuard:
    """Allows a message only if it is in `allowed`; raises for messages in `broken`."""

    def __init__(self, allowed=(), broken=()):
        self.allowed = set(allowed)
        self.broken = set(broken)

    def check(self, message, history):
        """Return an allow or refuse decision, or fail closed."""
        if message in self.broken:
            raise ScopeCheckError("down")
        category = "own_data" if message in self.allowed else "off_topic"
        return ScopeDecision(category=category, layer="classifier")


def test_expand_adds_every_mutation_of_every_attack_only():
    """Allow cases are not mutated; each attack gains one case per mutator, expecting a refusal."""
    cases = [case("a1"), case("a2", split="holdout"), case("ok1", expect="allow")]

    expanded = expand(cases)

    assert len(expanded) == 3 + 2 * len(MUTATORS)
    mutated = expanded[3:]
    assert {c.expect for c in mutated} == {"refuse"}
    assert {c.split for c in mutated} == {"dev", "holdout"}
    assert all(c.id.startswith(("a1+", "a2+")) for c in mutated)
    assert {c.family for c in mutated} == {f"mutation:{n}" for n in MUTATORS}


def test_a_perfect_guard_passes_the_gate():
    """Every attack is refused and every allow control is allowed."""
    cases = [case("a1", message="attack"), case("ok1", expect="allow", message="fine")]

    results = evaluate(cases, FakeGuard(allowed={"fine"}))

    assert all(r.passed for r in results) and gate(results) == []


def test_a_missed_core_case_fails_the_gate():
    """One core attack that gets through fails the run on its own."""
    cases = [case(f"a{i}", message=f"x{i}") for i in range(100)] + [
        case("core1", core=True, message="bad")
    ]

    problems = gate(evaluate(cases, FakeGuard(allowed={"bad"})))

    assert any("core case(s) failed: ['core1']" in p for p in problems)
    assert not any("attack block rate" in p for p in problems)  # 100/101 is above the floor


def test_low_attack_and_allow_rates_fail_the_gate():
    """Below the floor on either side fails, and holdout is reported separately."""
    attacks = [case(f"a{i}", split="holdout", message=f"x{i}") for i in range(10)]
    allows = [case(f"o{i}", expect="allow", split="holdout", message=f"y{i}") for i in range(10)]

    problems = gate(evaluate(attacks + allows, FakeGuard(allowed={"x0", "x1"})))

    text = " ".join(problems)
    assert "attack block rate 80.0%" in text and "attack block rate (holdout) 80.0%" in text
    assert "allow pass rate 0.0%" in text and "allow pass rate (holdout) 0.0%" in text
    assert MIN_RATE == 0.98


def test_a_guard_error_fails_the_gate_and_is_reported():
    """An errored case is an infrastructure failure, never a pass."""
    results = evaluate([case("a1", message="boom")], FakeGuard(broken={"boom"}))

    assert results[0].error == "down" and not results[0].passed
    assert any("errored" in p for p in gate(results))
    assert "FAIL a1 (refuse expected, got error)" in render(results)


def test_report_has_a_row_per_family_and_split_and_lists_failures():
    """The table names the family that broke."""
    cases = [
        case("a1", family="obfuscation", message="x"),
        case("a2", family="obfuscation", split="holdout", message="y"),
        case("ok1", expect="allow", family="allow", message="fine"),
    ]

    report = render(evaluate(cases, FakeGuard(allowed={"y", "fine"})))

    assert "obfuscation" in report and "1/1" in report and "0/1" in report
    assert "2/3 cases passed" in report
    assert "FAIL a2 (refuse expected, got allowed)" in report


def test_result_passed_requires_a_decision():
    """No decision never counts as a pass, even for an attack."""
    assert not Result(case("a1"), None).passed
    assert Result(case("a1"), False).passed and Result(case("o", expect="allow"), True).passed


def test_main_needs_an_api_key(monkeypatch, capsys):
    """Without a key the script exits with 2 and says why."""
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setattr("dotenv.find_dotenv", lambda **kw: "")

    assert main([]) == 2
    assert "OPENAI_API_KEY" in capsys.readouterr().out


def test_main_runs_the_corpus_and_returns_the_gate_status(monkeypatch, capsys):
    """With a fake guard that refuses everything, allow controls fail and the exit code is 1."""
    monkeypatch.setattr("dotenv.load_dotenv", lambda *args, **kwargs: False)
    monkeypatch.setenv("OPENAI_API_KEY", "sk-test")
    monkeypatch.setattr("agent.scope.default_scope_guard", lambda key: FakeGuard())

    assert main(["--no-mutations"]) == 1
    out = capsys.readouterr().out
    assert "GATE FAILED: allow pass rate" in out and "cases passed" in out
