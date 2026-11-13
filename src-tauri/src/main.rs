// Prevents an extra console window on Windows in release. Harmless on macOS,
// which is the primary (and, for now, only) target.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> tauri::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args
        .next()
        .is_some_and(|first| first == office_building_lib::manager::helper::MODE)
    {
        std::process::exit(office_building_lib::manager::helper::run(args.next()));
    }
    office_building_lib::run()
}
