//! Windows release builds must not allocate a console; debug builds keep it for `cargo run`.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() -> std::process::ExitCode {
    localme::run()
}
