//! Cross-language check: the ai-service mints `auth-token` cookies; the server must accept them.
//!
//! The value below is produced by `ai-service/agent/auth_cookie.py` for user 42, expiring at
//! 1900000300, with the key `bytes(range(64))` and the nonce `bytes(range(12))`. If this test
//! fails, one side changed its cookie format and the chat agent can no longer save changes.

use tower_cookies::cookie::{Cookie, CookieJar, Key};

use crate::middleware::parse_auth_token;

const PYTHON_MADE: &str = "AAECAwQFBgcICQoLKSE41WcB2cKNwPn2FkNuDyNAxxcDdsjSz6OkKMV67CLdSo7+pcS7";

fn key() -> Key {
    let bytes: Vec<u8> = (0..64u8).collect();
    Key::try_from(bytes.as_slice()).expect("64 bytes")
}

#[test]
fn server_decrypts_a_cookie_minted_by_the_ai_service() {
    let mut jar = CookieJar::new();
    jar.add_original(Cookie::new("auth-token", PYTHON_MADE));

    let decrypted = jar
        .private(&key())
        .get("auth-token")
        .expect("the server must accept a cookie minted by the ai-service");

    assert_eq!(decrypted.value(), "user-42.1900000300.sign");
    assert_eq!(
        parse_auth_token(decrypted.value()),
        Some((42, 1_900_000_300))
    );
}

#[test]
fn the_cookie_is_bound_to_its_name() {
    let mut jar = CookieJar::new();
    jar.add_original(Cookie::new("other-token", PYTHON_MADE));

    assert!(jar.private(&key()).get("other-token").is_none());
}

#[test]
fn a_different_key_cannot_decrypt_it() {
    let mut jar = CookieJar::new();
    jar.add_original(Cookie::new("auth-token", PYTHON_MADE));
    let other = Key::try_from(vec![7u8; 64].as_slice()).expect("64 bytes");

    assert!(jar.private(&other).get("auth-token").is_none());
}
