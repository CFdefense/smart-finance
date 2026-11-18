"""Mint the encrypted `auth-token` cookie that the Rust API's auth middleware accepts."""

import base64
import os

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

COOKIE_NAME = "auth-token"
DEFAULT_TTL_SECONDS = 300
_KEY_BYTES = 64
_NONCE_BYTES = 12


def _encryption_key(key_hex: str) -> bytes:
    """Return the AES key: bytes 32..64 of the hex-decoded cookie key (0..32 is the signing key)."""
    try:
        key = bytes.fromhex(key_hex.strip())
    except ValueError:
        raise ValueError("COOKIE_KEY must be hexadecimal") from None
    if len(key) != _KEY_BYTES:
        raise ValueError(f"COOKIE_KEY must be {_KEY_BYTES} bytes ({_KEY_BYTES * 2} hex characters)")
    return key[32:]


def mint_auth_cookie(
    user_id: int,
    key_hex: str,
    now: int,
    ttl: int = DEFAULT_TTL_SECONDS,
    nonce: bytes | None = None,
) -> str:
    """
    Return the cookie value for `user_id`, valid from `now` for `ttl` seconds.

    The value is `base64(nonce + AES-256-GCM(plaintext) + tag)` with the cookie name as the
    associated data, exactly what the Rust `cookie` crate's private jar produces and reads.
    """
    key = _encryption_key(key_hex)
    nonce = os.urandom(_NONCE_BYTES) if nonce is None else nonce
    if len(nonce) != _NONCE_BYTES:
        raise ValueError(f"nonce must be {_NONCE_BYTES} bytes")
    plaintext = f"user-{user_id}.{now + ttl}.sign".encode()
    sealed = AESGCM(key).encrypt(nonce, plaintext, COOKIE_NAME.encode())
    return base64.b64encode(nonce + sealed).decode("ascii")
