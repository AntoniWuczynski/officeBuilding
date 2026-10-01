// Prevents an extra console window on Windows in release. Harmless on macOS,
// which is the primary (and, for now, only) target.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> tauri::Result<()> {
    office_building_lib::run()
}
