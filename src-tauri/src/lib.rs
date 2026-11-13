//! Office Building: Tauri entry. Discovers coding-agent sessions from each
//! tool's own files, keeps them live, and serves them to the webview.

pub mod alerts;
pub mod commands;
pub mod connectors;
pub mod delivery;
pub mod discovery;
pub mod editor;
pub mod feed;
pub mod focus;
pub mod handover;
pub mod http;
pub mod manager;
pub mod model;
pub mod office;
pub mod options;
pub mod poll;
pub mod process;
pub mod project_files;
pub mod pty;
pub mod qa;
pub mod routing;
pub mod secrets;
pub mod settings;
pub mod setup;
pub mod store;
pub mod structured;
pub mod sync;
pub mod tools;
pub mod usage;
pub mod watch;

use alerts::Alerts;
use connectors::{Connector, hub::Hub, linear::Linear};
use office::{ManagerHost, Office};
use pty::{TerminalEvent, Terminals};
use settings::SettingsStore;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use tauri::{Emitter, Manager};

/// Push a snapshot from a background thread. A closed webview just means nobody is listening.
fn snapshot_pusher(app: &tauri::App) -> Box<dyn Fn() + Send + Sync> {
    let handle = app.handle().clone();
    Box::new(move || {
        if let Some(office) = handle.try_state::<Arc<Office>>() {
            let _ = handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
        }
    })
}

/// The pull request watch behind the QA desks, handed to Tauri. It pushes a snapshot when
/// anything it shows has changed.
fn qa_watch(
    app: &tauri::App,
    settings: &Arc<SettingsStore>,
    data: PathBuf,
) -> Arc<qa::watch::Watch> {
    let qa_watch = Arc::new(qa::watch::Watch::new(
        Arc::new(process::SystemRunner),
        Arc::clone(settings),
        data,
        snapshot_pusher(app),
    ));
    app.manage(Arc::clone(&qa_watch));
    qa_watch
}

/// The managers' socket in `<data>/ipc`, and the host the office runs managers with. A socket
/// that cannot be opened stops no other part of the app: its reason is kept, and a hire says it.
fn manager_host(
    app: &tauri::App,
    data: &std::path::Path,
) -> (Option<manager::socket::Server>, Result<ManagerHost, String>) {
    let server = match manager::socket::Server::bind(&data.join("ipc")) {
        Ok(server) => server,
        Err(reason) => return (None, Err(reason)),
    };
    let host = std::env::current_exe()
        .map_err(|e| format!("the app's own path is unknown ({e})"))
        .map(|helper| ManagerHost {
            registry: Arc::new(manager::auth::Registry::default()),
            socket: server.path().to_path_buf(),
            helper,
            changed: snapshot_pusher(app),
        });
    (Some(server), host)
}

/// The structured desks, telling the webview of every update. A desk whose state changed
/// redraws the floor, may alert, and may take a message that waits for it.
fn structured_desks(app: &tauri::App) -> Arc<structured::desks::Desks> {
    let handle = app.handle().clone();
    structured::desks::Desks::new(Arc::new(move |update| {
        // A closed webview just means nobody is watching; the desk keeps its transcript.
        let redraw = update.desk_changed;
        let _ = handle.emit(structured::EVENT, update);
        if redraw && let Some(office) = handle.try_state::<Arc<Office>>() {
            let _ = handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
            // The watch thread's next pass flushes anything this queues, at most a tick late.
            if let Some(alerts) = handle.try_state::<Arc<Alerts>>() {
                let _ = alerts.check(&handle, &office);
            }
            // On its own thread: a hand-over prompts a driver, and this callback can run
            // inside a driver's own call, which holds the drivers' lock (`Desks::on_event`).
            let office = Arc::clone(&office);
            let handle = handle.clone();
            // A thread that cannot start leaves the message for the watch loop's next pass.
            let _ = std::thread::Builder::new()
                .name("deliver-structured".to_string())
                .spawn(move || {
                    if office.deliver_due() {
                        let _ = handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
                    }
                });
        }
    }))
}

/// Open the stores, start discovery and the watchers, and hand Tauri the shared state.
fn set_up(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("the home folder could not be found")?;
    let data = app.path().app_data_dir()?;
    // `Store::open` does not make the folder, and the socket's folder goes inside it.
    std::fs::create_dir_all(&data)?;
    let store = Store::open(data.join("store.json"))?;
    let settings = Arc::new(SettingsStore::open(data.join("settings.json")));
    let alerts = Arc::new(Alerts::new(Arc::clone(&settings)));
    let http: Arc<dyn http::HttpClient> = Arc::new(http::UreqClient::new(Duration::from_secs(15)));
    let keychain: Arc<dyn secrets::SecretStore> =
        Arc::new(secrets::Keychain::new(app.config().identifier.clone()));
    let keys = Arc::new(routing::keys::Keys::new(Arc::clone(&keychain)));
    app.manage(Arc::clone(&keys));
    let router = routing::router::Router::new(
        routing::records::ExhaustionFile::new(data.join("routing-exhaustion.json")),
        keys,
        Arc::new(routing::upstream::UreqUpstream::new()),
        routing::upstream::Bases::default(),
    );
    app.manage(Arc::clone(&router));
    let linear = Arc::new(Linear::new(http, keychain, Arc::clone(&settings)));
    let linear_connector: Arc<dyn Connector> = linear.clone();
    let hub = Arc::new(Hub::new(vec![linear_connector], snapshot_pusher(app)));
    app.manage(Arc::clone(&hub));
    app.manage(linear);
    app.manage(Arc::clone(&settings));
    app.manage(Arc::new(editor::Editors::new(
        Arc::new(process::SystemRunner),
        vec![PathBuf::from("/Applications"), home.join("Applications")],
    )));
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
                let manager_gone = office.as_ref().is_some_and(|o| o.manager_left(&e));
                let failed = office.as_ref().and_then(|o| o.hire_failed(&e));
                let shell_gone = office.as_ref().is_some_and(|o| o.shell_exited(&e));
                if let Some(router) = handle.try_state::<Arc<routing::router::Router>>() {
                    router.terminal_exited(&e.terminal_id);
                }
                let _ = handle.emit(pty::EXIT_EVENT, e);
                if let Some(failed) = failed {
                    let _ = handle.emit(office::HIRE_FAILED_EVENT, failed);
                }
                if let (true, Some(office)) = (shell_gone || manager_gone, office) {
                    let _ = handle.emit(watch::SNAPSHOT_EVENT, office.snapshot());
                }
            }
        }
    }));
    let watched_setup = setup.watched(&home);
    let feed = feed::Feed::new(data.join("feed"));
    std::fs::create_dir_all(feed.dir())?;
    let feed_dir = feed.dir().to_path_buf();
    let structured = structured_desks(app);
    let qa_watch = qa_watch(app, &settings, data.clone());
    let (server, host) = manager_host(app, &data);
    let registry = host.as_ref().ok().map(|h| Arc::clone(&h.registry));
    let office = Arc::new(
        Office::new(home.clone(), store, terminals, setup, feed, structured)
            .with_connectors(Arc::clone(&hub))
            .with_settings(Arc::clone(&settings))
            .with_qa(Arc::clone(&qa_watch))
            .with_manager(host)
            .with_routing(router)
            .with_handover(office::HandoverHost::system(&data)),
    );
    if let (Some(server), Some(registry)) = (server, registry) {
        server.serve(
            registry,
            Arc::new(manager::mcp::Protocol::new(office.clone())),
        )?;
    }
    office.refresh();
    office.ensure_setup_floor()?;
    watch::start_setup(app.handle().clone(), watched_setup)?;
    app.manage(office.clone());
    let polled = Arc::clone(&office);
    let reviewed: Arc<dyn qa::watch::Building> = office.clone();
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
    connectors::hub::start(&hub, move || polled.linked_floors())?;
    qa::watch::start(&qa_watch, reviewed)?;
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
            commands::editors,
            commands::open_in_editor,
            commands::linear_key_status,
            commands::set_linear_key,
            commands::linear_teams,
            commands::connect_floor,
            commands::disconnect_floor,
            commands::recheck_connectors,
            commands::open_todo_link,
            commands::set_floor_qa,
            commands::open_review,
            commands::recheck_qa,
            commands::hire_manager,
            commands::set_manager_paused,
            commands::set_manager_hiring,
            commands::withdraw_manager_note,
            commands::withdraw_message,
            routing::commands::routing_key_status,
            routing::commands::set_routing_key,
            routing::commands::floor_routing,
            routing::commands::set_floor_context,
            routing::commands::desk_route,
            routing::commands::retry_route,
            routing::commands::switch_route,
            handover::commands::hand_over_desk,
        ])
        .run(tauri::generate_context!())
}
