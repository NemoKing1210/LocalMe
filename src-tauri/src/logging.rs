//! Logging setup.
//!
//! One subscriber for the whole process, writing to stderr so that on Windows the console
//! build shows it and on macOS/Linux it lands in the launch terminal. Verbosity is
//! controlled by `RUST_LOG`, defaulting to `info` for our own crates and `warn` for the
//! dependency tree, which keeps a normal run quiet without hiding a real problem.

use tracing_subscriber::EnvFilter;

/// Installs the global subscriber.
///
/// Called once, first thing in [`crate::run`]. A second call is ignored by
/// `tracing_subscriber`, and a malformed `RUST_LOG` falls back to the default filter rather
/// than failing the start.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("localme=info,localme_core=info,tauri=warn,tauri_runtime=warn,mdns_sd=warn")
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}
