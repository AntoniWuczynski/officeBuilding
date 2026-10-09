//! Office Building: Tauri entry. Discovers coding-agent sessions from each
//! tool's own files, keeps them live, and serves them to the webview.

pub mod commands;
pub mod discovery;
pub mod feed;
pub mod focus;
pub mod model;
pub mod office;
pub mod options;
pub mod project_files;
pub mod pty;
pub mod setup;
pub mod store;
pub mod structured;
pub mod tools;
pub mod usage;
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
            let data = app.path().app_data_dir()?;
            let store = Store::open(data.join("store.json"))?;
            let setup = setup::Library::new(data.join("Setup"), data.join("setup-build"));
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
            let watched_setup = setup.watched(&home);
            let feed = feed::Feed::new(data.join("feed"));
            std::fs::create_dir_all(feed.dir())?;
            let feed_dir = feed.dir().to_path_buf();
            let structured_handle = app.handle().clone();
            let structured = structured::desks::Desks::new(Arc::new(move |update| {
                // A closed webview just means nobody is watching; the desk keeps its transcript.
                let redraw = update.desk_changed;
                let _ = structured_handle.emit(structured::EVENT, update);
                if redraw && let Some(office) = structured_handle.try_state::<Arc<Office>>() {
                    let _ = structured_handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
                }
            }));
            let office = Arc::new(Office::new(
                home.clone(),
                store,
                terminals,
                setup,
                feed,
                structured,
            ));
            office.refresh();
            office.ensure_setup_floor()?;
            watch::start_setup(app.handle().clone(), watched_setup)?;
            app.manage(office.clone());
            watch::start(
                app.handle().clone(),
                office,
                &[
                    home.join(".claude/projects"),
                    home.join(".claude/sessions"),
                    home.join(".codex/sessions"),
                    home.join(".codex/session_index.jsonl"),
                    feed_dir,
                ],
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::hire_options,
            commands::tool_usage,
            commands::setup_view,
            commands::set_setup_targets,
            commands::import_setup_item,
            commands::structured_backlog,
            commands::structured_prompt,
            commands::structured_answer,
            commands::structured_cancel,
            commands::structured_stop,
            commands::spawn_session,
            commands::resume_session,
            commands::answer_question,
            commands::tick_todo,
            commands::send_agent_message,
            commands::focus_session,
            commands::add_floor,
            commands::demolish_floor,
            commands::pick_folder,
            commands::terminal_backlog,
            commands::terminal_write,
            commands::terminal_resize,
        ])
        .run(tauri::generate_context!())
}
