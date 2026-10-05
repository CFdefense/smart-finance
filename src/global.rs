//! Global compile-time constants shared across the application.

/// Directory where log files are written.
pub const LOG_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/logs");

/// File name for the crash log written on panic.
pub const CRASH_LOG: &str = "crash.log";

/// File name for the rolling application log.
pub const LATEST_LOG: &str = "latest.log";
