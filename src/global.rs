//! Global compile-time constants shared across the application.

/// Directory where log files are written.
pub const LOG_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/logs");

/// File name for the crash log written on panic.
pub const CRASH_LOG: &str = "crash.log";

/// File name for the rolling application log.
pub const LATEST_LOG: &str = "latest.log";

/// Session duration in seconds (3 days). Used for both cookie issuance and refresh.
pub const SESSION_DURATION_SECS: i64 = 60 * 60 * 24 * 3; // 3 days

/// Largest accepted monetary amount (guards against absurd values and float overflow).
pub const MAX_AMOUNT: f64 = 1_000_000_000.0;

/// Tolerance when checking that the manual total stays non-negative (half a cent).
pub const EPSILON: f64 = 0.005;

/// Maximum length of a goal description.
pub const GOAL_MAX_DESCRIPTION: usize = 500;

/// Maximum length of a contribution note.
pub const GOAL_MAX_NOTE: usize = 200;