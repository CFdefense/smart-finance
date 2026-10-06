//! Logger initialisation and panic handler.

use crate::global::{CRASH_LOG, LATEST_LOG, LOG_DIR};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    sync::Once,
};
use tracing::error;
use tracing_appender::rolling;
use tracing_subscriber::{
    fmt::time::SystemTime, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

static INIT_LOG: Once = Once::new();

/// Installs a panic hook that writes a crash log to `logs/crash.log`.
///
/// Sets `RUST_BACKTRACE=full` so the captured backtrace is complete.
///
/// # Panics
///
/// Panics if the log directory cannot be created or the crash log file cannot be written.
pub fn init_panic_handler() {
    std::env::set_var("RUST_BACKTRACE", "full");
    std::panic::set_hook(Box::new(move |panic_info| {
        const WRITE_ERR: &str = "Could not write to crash log";
        error!("{}", panic_info);
        println!("{panic_info}");

        fs::create_dir_all(LOG_DIR).expect("Could not create log dir for crash log");
        let file = File::create(Path::new(LOG_DIR).join(CRASH_LOG))
            .expect("Could not create crash log file");
        let backtrace = std::backtrace::Backtrace::capture();
        let mut writer = BufWriter::new(file);

        writeln!(writer, "Time: {}", chrono::Local::now()).expect(WRITE_ERR);
        writeln!(writer, "{panic_info}").expect(WRITE_ERR);
        writeln!(writer, "stack backtrace:\n{backtrace}").expect(WRITE_ERR);
        writeln!(writer, "Process finished with exit code 101").expect(WRITE_ERR);
        writer.flush().expect(WRITE_ERR);
    }));
}

/// Initialises the tracing subscriber.
///
/// Installs two layers:
/// - A file layer writing pretty-formatted logs to `logs/latest.log`, filtered by `RUST_LOG`.
/// - A stdout layer writing compact ANSI-coloured logs, filtered by `RUST_LOG`.
///
/// Safe to call multiple times — only the first call takes effect.
pub fn init_logger() {
    INIT_LOG.call_once(|| {
        // Remove stale log from previous run
        let _ = fs::remove_file(Path::new(LOG_DIR).join(LATEST_LOG));

        let (log_writer, log_guard) =
            tracing_appender::non_blocking(rolling::never(LOG_DIR, LATEST_LOG));

        let file_layer = tracing_subscriber::fmt::layer()
            .with_timer(SystemTime)
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_level(true)
            .with_thread_names(true)
            .with_thread_ids(true)
            .pretty()
            .with_writer(log_writer)
            .with_filter(EnvFilter::from_default_env());

        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_timer(SystemTime)
            .compact()
            .with_filter(EnvFilter::from_default_env());

        tracing_subscriber::registry()
            .with(file_layer)
            .with(stdout_layer)
            .init();

        // Guard must be 'static — leak it so the background writer stays alive.
        Box::leak(Box::new(log_guard));
    });
}
