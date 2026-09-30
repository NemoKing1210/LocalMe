//! Build script for the Tauri host.
//!
//! `tauri-build` reads `tauri.conf.json`, validates the capability files and generates the
//! context `tauri::generate_context!()` expands to. It also emits the JSON schema of every
//! permission a plugin contributes, into `gen/schemas`, which is what makes the capability
//! files below type-checked rather than stringly-typed.

fn main() {
    tauri_build::build();
}
