//! Validation helpers and serde utilities shared by budget, goal and rule models.

use serde::{Deserialize, Deserializer};

/// Largest accepted monetary amount (guards against absurd values and float overflow).
pub const MAX_AMOUNT: f64 = 1_000_000_000.0;

/// Rounds a monetary value to cents to hide floating-point noise.
pub fn round_cents(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Validates a display name (1–100 characters after trimming).
pub fn validate_name(name: &str) -> Result<(), String> {
    let len = name.trim().chars().count();
    if len == 0 || len > 100 {
        return Err("name must be between 1 and 100 characters".to_string());
    }
    Ok(())
}

/// Validates that `value` has at most `max` characters after trimming.
pub fn validate_max_len(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().chars().count() > max {
        return Err(format!("{field} must be at most {max} characters"));
    }
    Ok(())
}

/// Validates a strictly positive, finite amount no larger than [`MAX_AMOUNT`].
pub fn validate_positive_amount(field: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 || value > MAX_AMOUNT {
        return Err(format!(
            "{field} must be greater than 0 and at most {MAX_AMOUNT}"
        ));
    }
    Ok(())
}

/// Serde helper distinguishing an absent field (`None`) from an explicit `null` (`Some(None)`).
///
/// Use with `#[serde(default, deserialize_with = "double_option")]` on `Option<Option<T>>`.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Deserialize)]
    struct Patch {
        #[serde(default, deserialize_with = "double_option")]
        value: Option<Option<String>>,
    }

    #[test]
    fn double_option_distinguishes_absent_null_and_value() {
        let absent: Patch = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.value, None);
        let null: Patch = serde_json::from_str(r#"{"value":null}"#).unwrap();
        assert_eq!(null.value, Some(None));
        let set: Patch = serde_json::from_str(r#"{"value":"x"}"#).unwrap();
        assert_eq!(set.value, Some(Some("x".to_string())));
    }

    #[test]
    fn max_len_counts_trimmed_characters() {
        assert!(validate_max_len("note", "  abc  ", 3).is_ok());
        assert!(validate_max_len("note", "abcd", 3).is_err());
    }

    #[test]
    fn round_cents_hides_float_noise() {
        assert_eq!(round_cents(0.1 + 0.2), 0.3);
    }
}
