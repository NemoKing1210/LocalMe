//! The binary entry point.
//!
//! On Windows a release build must not allocate a console window, which is what the
//! `windows_subsystem` attribute below is for; debug builds keep the console so that
//! `cargo run` shows the log.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() -> std::process::ExitCode {
    localme::run()
}
