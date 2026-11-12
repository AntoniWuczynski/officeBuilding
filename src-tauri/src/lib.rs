//! Office Building: Tauri entry. Discovers coding-agent sessions from each
//! tool's own files, keeps them live, and serves them to the webview.

pub mod alerts;
pub mod commands;
pub mod discovery;
pub mod feed;
pub mod focus;
pub mod model;
pub mod office;
pub mod options;
pub mod project_files;
pub mod pty;
pub mod settings;
pub mod setup;
pub mod store;
pub mod structured;
pub mod sync;
pub mod tools;
pub mod usage;
pub mod watch;

use alerts::Alerts;
use office::Office;
use pty::{TerminalEvent, Terminals};
use settings::SettingsStore;
use std::sync::Arc;
use store::Store;
use tauri::{Emitter, Manager};

/// Open the stores, start discovery and the watchers, and hand Tauri the shared state.
fn set_up(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("the home folder could not be found")?;
    let data = app.path().app_data_dir()?;
    let store = Store::open(data.join("store.json"))?;
    let settings = Arc::new(SettingsStore::open(data.join("settings.json")));
    let alerts = Arc::new(Alerts::new(Arc::clone(&settings)));
    app.manage(settings);
    app.manage(Arc::clone(&alerts));
    let setup = setup::Library::new(data.join("Setup"), data.join("setup-build"));
    let handle = app.handle().clone();
    let terminals = Terminals::new(Arc::new(move |event| {
        // A closed webview just means nobody is watching; the scrollback keeps the output.
        match event {
            TerminalEvent::Output(o) => {
                let _ = handle.emit(pty::OUTPUT_EVENT, o);
            }
            TerminalEvent::Exit(e) => {
                let office = handle.try_state::<Arc<Office>>();
                let failed = office.as_ref().and_then(|o| o.hire_failed(&e));
                let shell_gone = office.as_ref().is_some_and(|o| o.shell_exited(&e));
                let _ = handle.emit(pty::EXIT_EVENT, e);
                if let Some(failed) = failed {
                    let _ = handle.emit(office::HIRE_FAILED_EVENT, failed);
                }
                if let (true, Some(office)) = (shell_gone, office) {
                    let _ = handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
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
            // The watch thread's next pass flushes anything this queues, at most a tick late.
            if let Some(alerts) = structured_handle.try_state::<Arc<Alerts>>() {
                let _ = alerts.check(&structured_handle, &office);
            }
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
        alerts,
        &[
            home.join(".claude/projects"),
            home.join(".claude/sessions"),
            home.join(".codex/sessions"),
            home.join(".codex/session_index.jsonl"),
            feed_dir,
        ],
    )?;
    Ok(())
}

/// Start the app.
///
/// # Errors
/// When Tauri cannot start or run the window.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(set_up)
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Focus is followed from events, never asked for: `is_focused` blocks on the main thread.
            if let tauri::WindowEvent::Focused(focused) = event
                && let Some(alerts) = window.try_state::<Arc<Alerts>>()
            {
                alerts.set_focused(*focused);
            }
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
            commands::open_shell,
            commands::end_shell,
            commands::terminal_backlog,
            commands::terminal_write,
            commands::terminal_resize,
            commands::get_settings,
            commands::update_settings,
            commands::set_open_desk,
            commands::test_alert,
        ])
        .run(tauri::generate_context!())
}
