"""Tests for the in-memory store of proposed (unconfirmed) changes."""

import threading

import pytest

from agent.pending import PendingActionStore, PendingError


class Clock:
    """A clock tests can move by hand."""

    def __init__(self):
        self.now = 1000.0

    def __call__(self) -> float:
        """Return the current fake time."""
        return self.now


@pytest.fixture
def clock():
    """A movable clock."""
    return Clock()


@pytest.fixture
def store(clock):
    """A store with the default limits and the fake clock."""
    return PendingActionStore(clock=clock)


def add(store, user_id=7, request_id="r1", preview="Create budget"):
    """Stage one action."""
    return store.add(user_id, "budget", {"method": "POST"}, preview, request_id)


def test_add_returns_an_action_with_a_short_id(store):
    """Each action gets an 8-character hex reference and keeps what was staged."""
    action = add(store)
    assert len(action.action_id) == 8 and int(action.action_id, 16) >= 0
    assert (action.user_id, action.kind, action.request_id) == (7, "budget", "r1")
    assert action.payload == {"method": "POST"} and action.preview == "Create budget"
    assert len(store) == 1


def test_ids_are_unique(store):
    """Two actions never share a reference."""
    assert add(store).action_id != add(store).action_id


def test_take_by_id_in_a_later_request(store):
    """A later request can take the action by its reference."""
    action = add(store)
    assert store.take(7, "r2", action.action_id) == action
    assert len(store) == 0


def test_take_without_id_uses_the_only_pending_action(store):
    """With exactly one pending action, no reference is needed."""
    action = add(store)
    assert store.take(7, "r2").action_id == action.action_id


def test_take_without_id_and_nothing_pending(store):
    """Nothing staged is reported plainly."""
    with pytest.raises(PendingError, match="nothing"):
        store.take(7, "r2")


def test_take_without_id_and_several_pending_lists_them(store):
    """The model must pick one when several are waiting."""
    a, b = add(store), add(store)
    with pytest.raises(PendingError) as err:
        store.take(7, "r2")
    assert a.action_id in str(err.value) and b.action_id in str(err.value)
    assert len(store) == 2


def test_action_is_single_use(store):
    """Taking an action removes it, so it cannot be replayed."""
    action = add(store)
    store.take(7, "r2", action.action_id)
    with pytest.raises(PendingError, match="no pending change"):
        store.take(7, "r3", action.action_id)


def test_same_request_is_refused_and_keeps_the_action(store):
    """Proposing and confirming in one request is refused; the action stays for later."""
    action = add(store, request_id="r1")
    with pytest.raises(PendingError, match="same turn"):
        store.take(7, "r1", action.action_id)
    assert store.take(7, "r2", action.action_id) == action


def test_same_request_is_refused_without_an_id_too(store):
    """The same-request rule also applies when the reference is omitted."""
    add(store, request_id="r1")
    with pytest.raises(PendingError, match="same turn"):
        store.take(7, "r1")


def test_other_users_cannot_take_an_action(store):
    """An action belongs to the user who proposed it and looks unknown to everyone else."""
    action = add(store, user_id=7)
    with pytest.raises(PendingError, match="no pending change"):
        store.take(8, "r2", action.action_id)
    assert len(store) == 1


def test_other_users_do_not_see_each_others_pending_without_an_id(store):
    """Taking without an id only considers the caller's own actions."""
    add(store, user_id=7)
    with pytest.raises(PendingError, match="nothing"):
        store.take(8, "r2")


def test_unknown_id(store):
    """A made-up reference is refused."""
    with pytest.raises(PendingError, match="no pending change"):
        store.take(7, "r2", "deadbeef")


def test_id_is_trimmed(store):
    """Whitespace the model adds around a reference is ignored."""
    action = add(store)
    assert store.take(7, "r2", f"  {action.action_id}\n") == action


def test_actions_expire_after_the_ttl(store, clock):
    """After 600 seconds the action is gone and the message says it may have expired."""
    action = add(store)
    clock.now += 601
    with pytest.raises(PendingError, match="expired"):
        store.take(7, "r2", action.action_id)
    assert len(store) == 0


def test_actions_are_valid_just_before_the_ttl(store, clock):
    """An action is still valid at 599 seconds."""
    action = add(store)
    clock.now += 599
    assert store.take(7, "r2", action.action_id) == action


def test_capacity_drops_the_oldest(clock):
    """A full store makes room by dropping its oldest action."""
    store = PendingActionStore(capacity=2, clock=clock)
    first, second, third = add(store), add(store), add(store)
    assert len(store) == 2
    with pytest.raises(PendingError):
        store.take(7, "r2", first.action_id)
    assert store.take(7, "r2", second.action_id) == second
    assert store.take(7, "r2", third.action_id) == third


def test_expired_actions_are_purged_before_capacity_is_applied(clock):
    """Expired actions do not count against capacity."""
    store = PendingActionStore(capacity=1, ttl=10, clock=clock)
    add(store)
    clock.now += 11
    fresh = add(store)
    assert store.take(7, "r2", fresh.action_id) == fresh


def test_clear_empties_the_store(store):
    """clear() forgets everything (used between eval cases)."""
    add(store)
    store.clear()
    assert len(store) == 0


def test_concurrent_takes_confirm_only_once(store):
    """Racing confirmations cannot both succeed."""
    action = add(store)
    winners = []

    def race():
        try:
            winners.append(store.take(7, "r2", action.action_id))
        except PendingError:
            pass

    threads = [threading.Thread(target=race) for _ in range(20)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert len(winners) == 1


def test_one_users_expiry_is_never_reported_to_another_user(store, clock):
    """A user with nothing pending is told 'nothing waiting', not that something expired."""
    add(store, user_id=7)
    clock.now += 601
    with pytest.raises(PendingError, match="nothing waiting") as other:
        store.take(8, "r2")
    assert "expired" not in str(other.value)


def test_len_ignores_expired_actions(store, clock):
    """The store size does not count actions that have timed out."""
    add(store)
    clock.now += 601
    assert len(store) == 0
