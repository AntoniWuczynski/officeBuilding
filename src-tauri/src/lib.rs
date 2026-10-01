//! Office Building: Tauri entry. Discovers coding-agent sessions from each
//! tool's own files, keeps them live, and serves them to the webview.

pub mod commands;
pub mod discovery;
pub mod focus;
pub mod model;
pub mod office;
pub mod options;
pub mod project_files;
pub mod pty;
pub mod store;
pub mod watch;

use office::Office;
use pty::{TerminalEvent, Terminals};
use std::sync::Arc;
use store::Store;
use tauri::{Emitter, Manager};

/// Start the app.
///
/// # Errors
/// When Tauri cannot start or run the window.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            let home = dirs::home_dir().ok_or("the home folder could not be found")?;
            let store = Store::open(app.path().app_data_dir()?.join("store.json"))?;
            let handle = app.handle().clone();
            let terminals = Terminals::new(Arc::new(move |event| {
                // A closed webview just means nobody is watching; the scrollback keeps the output.
                match event {
                    TerminalEvent::Output(o) => {
                        let _ = handle.emit(pty::OUTPUT_EVENT, o);
                    }
                    TerminalEvent::Exit(e) => {
                        let failed = handle
                            .try_state::<Arc<Office>>()
                            .and_then(|office| office.hire_failed(&e));
                        let _ = handle.emit(pty::EXIT_EVENT, e);
                        if let Some(failed) = failed {
                            let _ = handle.emit(office::HIRE_FAILED_EVENT, failed);
                        }
                    }
                }
            }));
            let office = Arc::new(Office::new(home.clone(), store, terminals));
            office.refresh();
            app.manage(office.clone());
            watch::start(
                app.handle().clone(),
                office,
                &[
                    home.join(".claude/projects"),
                    home.join(".claude/sessions"),
                    home.join(".codex/sessions"),
                    home.join(".codex/session_index.jsonl"),
                ],
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::hire_options,
            commands::spawn_session,
            commands::resume_session,
            commands::answer_question,
            commands::tick_todo,
            commands::send_agent_message,
            commands::focus_session,
            commands::add_floor,
            commands::pick_folder,
            commands::terminal_backlog,
            commands::terminal_write,
            commands::terminal_resize,
        ])
        .run(tauri::generate_context!())
}
