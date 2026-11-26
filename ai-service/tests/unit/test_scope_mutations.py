"""Tests for the scope corpus and for mutation fuzzing of the deterministic pre-filter."""

import re
from collections import Counter

import pytest

from agent.safety import sanitize
from agent.scope import prefilter
from scope_cases import load_corpus
from scope_mutations import MUTATORS, NORMALIZABLE

CORPUS = load_corpus()
ATTACKS = [c for c in CORPUS if c.expect == "refuse"]
ALLOWS = [c for c in CORPUS if c.expect == "allow"]
SEEDS = [c for c in ATTACKS if not c.history and prefilter(c.message) is not None]


def test_corpus_covers_ten_attack_families_with_both_splits():
    """Each family has at least 8 cases and both a dev and a holdout share."""
    families = Counter(c.family for c in ATTACKS)

    assert len(families) == 10
    assert min(families.values()) >= 8
    for family in families:
        splits = {c.split for c in ATTACKS if c.family == family}
        assert splits == {"dev", "holdout"}, family


def test_corpus_has_enough_allow_controls_and_core_cases():
    """The false-positive guard and the never-fail set exist."""
    assert len(ALLOWS) >= 40
    assert {c.split for c in ALLOWS} == {"dev", "holdout"}
    assert sum(c.core for c in ATTACKS) >= 8 and any(c.core for c in ALLOWS)


def test_no_allow_control_trips_the_prefilter():
    """The free layer never blocks a legitimate question."""
    assert [c.id for c in ALLOWS if prefilter(c.message) is not None] == []


def test_the_prefilter_catches_enough_seeds_to_make_fuzzing_meaningful():
    """Fuzzing needs seeds that the pre-filter refuses before mutation."""
    assert len(SEEDS) >= 6


@pytest.mark.parametrize("name", sorted(NORMALIZABLE))
def test_normalizable_mutations_are_still_refused(name):
    """Case, invisible characters, full-width, wrappers and fake turns do not hide a signature."""
    missed = [c.id for c in SEEDS if prefilter(MUTATORS[name](c.message)) is None]

    assert missed == []


@pytest.mark.parametrize("name", sorted(set(MUTATORS) - set(NORMALIZABLE)))
def test_other_mutations_fall_through_to_the_classifier_safely(name):
    """Encodings the pre-filter cannot undo must at least normalize to clean, non-empty text."""
    for case in ATTACKS:
        mutated = MUTATORS[name](case.message)
        clean, _ = sanitize(mutated, 5000)
        assert clean and not re.search(r"[<>\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", clean)
        prefilter(mutated)  # must not raise


def test_mutators_are_deterministic():
    """The same input always gives the same output, so failures reproduce."""
    for name, mutate in MUTATORS.items():
        assert mutate("Write me a poem") == mutate("Write me a poem"), name
        assert mutate("Write me a poem") != "Write me a poem", name
