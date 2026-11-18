"""Tests for minting the encrypted auth-token cookie the Rust API accepts."""

import base64

import pytest
from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from agent.auth_cookie import COOKIE_NAME, DEFAULT_TTL_SECONDS, mint_auth_cookie

KEY = bytes(range(64))
KEY_HEX = KEY.hex()
NONCE = bytes(range(12))
KNOWN_ANSWER = "AAECAwQFBgcICQoLKSE41WcB2cKNwPn2FkNuDyNAxxcDdsjSz6OkKMV67CLdSo7+pcS7"
RUST_PRODUCED = "rRbEBpE1SuxDQ7L4URiDpgpnQ8TKq3vBc/ygJLjnw7+islvX3JfU/6GerbKhqp4V90TS"


def open_cookie(value: str, aad: bytes = b"auth-token") -> str:
    """Decrypt a cookie value the way the Rust `cookie` crate does."""
    raw = base64.b64decode(value)
    return AESGCM(KEY[32:]).decrypt(raw[:12], raw[12:], aad).decode()


def test_matches_the_known_answer_vector():
    """A fixed nonce gives exactly the value the Rust side was verified to decrypt."""
    assert mint_auth_cookie(42, KEY_HEX, now=1_900_000_000, nonce=NONCE) == KNOWN_ANSWER


def test_decrypts_a_value_produced_by_rust():
    """The Rust-produced vector decrypts with the same key split and AAD."""
    assert open_cookie(RUST_PRODUCED) == "user-42.1900000300.sign"


def test_plaintext_has_user_expiry_and_sign_marker():
    """The plaintext is user-<id>.<now+ttl>.sign."""
    value = mint_auth_cookie(7, KEY_HEX, now=1000, ttl=60)
    assert open_cookie(value) == "user-7.1060.sign"


def test_default_ttl_is_five_minutes():
    """The default lifetime is 300 seconds."""
    assert DEFAULT_TTL_SECONDS == 300
    assert open_cookie(mint_auth_cookie(7, KEY_HEX, now=10)) == "user-7.310.sign"


def test_cookie_name_is_the_auth_token():
    """The AAD is the cookie name the middleware reads."""
    assert COOKIE_NAME == "auth-token"


def test_wrong_cookie_name_does_not_decrypt():
    """The cookie name is bound into the ciphertext."""
    value = mint_auth_cookie(7, KEY_HEX, now=1)
    with pytest.raises(InvalidTag):
        open_cookie(value, aad=b"other")


def test_tampered_value_does_not_decrypt():
    """Changing the last character breaks authentication."""
    value = mint_auth_cookie(7, KEY_HEX, now=1, nonce=NONCE)
    swapped = value[:-2] + ("A" if value[-2] != "A" else "B") + value[-1]
    with pytest.raises(InvalidTag):
        open_cookie(swapped)


def test_nonce_is_random_by_default():
    """Two cookies for the same user and time differ."""
    assert mint_auth_cookie(7, KEY_HEX, now=1) != mint_auth_cookie(7, KEY_HEX, now=1)


def test_different_users_get_different_plaintext():
    """The user id is part of what is encrypted."""
    a = open_cookie(mint_auth_cookie(1, KEY_HEX, now=1))
    b = open_cookie(mint_auth_cookie(2, KEY_HEX, now=1))
    assert a.startswith("user-1.") and b.startswith("user-2.")


@pytest.mark.parametrize("bad", ["", "zz", "00" * 10, "00" * 65, "not hex at all"])
def test_rejects_a_bad_key(bad):
    """A key that is not 64 hex-decoded bytes is refused with a clear error."""
    with pytest.raises(ValueError, match="COOKIE_KEY"):
        mint_auth_cookie(7, bad, now=1)


def test_rejects_a_bad_nonce_length():
    """AES-GCM here always uses a 12-byte nonce."""
    with pytest.raises(ValueError, match="nonce"):
        mint_auth_cookie(7, KEY_HEX, now=1, nonce=b"short")


def test_key_is_trimmed():
    """Whitespace around the hex key (from a .env file) is ignored."""
    assert mint_auth_cookie(42, f" {KEY_HEX}\n", now=1_900_000_000, nonce=NONCE) == KNOWN_ANSWER
