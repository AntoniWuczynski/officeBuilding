//! Tauri commands: the Rust side of `DiscoveryApi` (src/api/DiscoveryApi.ts).
//! The webview calls them with `invoke("<name>", { camelCaseArgs })`.
#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri's command macro hands each command owned, deserialized arguments and `State` by value"
)]

use crate::alerts::Alerts;
use crate::connectors::{self, hub::Hub, linear::Linear};
use crate::editor::{Editor, EditorList, Editors};
use crate::model::{
    ConnectorKind, HireRequest, LinearKeyStatus, LinearTeam, Link, OfficeSnapshot, Place, Project,
    QuestionAnswer, Session, Shell, TerminalBacklog, ToolKind, ToolOptions, ToolUsage,
};
use crate::office::Office;
use crate::process::{Runner, SystemRunner, browser_call};
use crate::qa::watch::Watch;
use crate::settings::{Settings, SettingsPatch, SettingsStore};
use crate::setup::{ItemKind, SetupView, Targets};
use crate::structured::desks::Backlog;
use crate::watch::SNAPSHOT_EVENT;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type OfficeState<'a> = State<'a, Arc<Office>>;
type HubState<'a> = State<'a, Arc<Hub>>;
type LinearState<'a> = State<'a, Arc<Linear>>;
type WatchState<'a> = State<'a, Arc<Watch>>;

/// How long `open` has to hand a link to the browser.
const OPEN_WAIT: Duration = Duration::from_secs(5);

/// Push the snapshot after a change the command rescanned itself: the watcher
/// only pushes what its own rescan finds changed, so it would never send this.
fn push_snapshot(app: &AppHandle, office: &Office) {
    // A closed webview just means nobody is listening right now.
    let _ = app.emit(SNAPSHOT_EVENT, office.snapshot());
}

#[tauri::command]
#[must_use]
pub fn get_snapshot(office: OfficeState<'_>) -> OfficeSnapshot {
    office.snapshot()
}

/// On a blocking thread: it runs `agy models` and a login shell, which take seconds.
#[tauri::command]
pub async fn hire_options(office: OfficeState<'_>) -> Result<Vec<ToolOptions>, String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.hire_options())
        .await
        .map_err(|e| format!("the hire options did not load: {e}"))
}

/// On a blocking thread: finding the installed tools spawns a login shell, and
/// `refresh` has Claude Code check its usage before the files are read.
#[tauri::command]
pub async fn tool_usage(refresh: bool) -> Result<Vec<ToolUsage>, String> {
    let home = dirs::home_dir().ok_or("the home folder is unknown")?;
    tauri::async_runtime::spawn_blocking(move || {
        let installed = crate::tools::installed();
        if refresh && installed.contains(&ToolKind::ClaudeCode) {
            crate::usage::refresh_claude()?;
        }
        Ok(crate::usage::tool_usage(&home, &installed))
    })
    .await
    .map_err(|e| format!("the usage check did not finish: {e}"))?
}

/// On a blocking thread: finding the installed tools spawns a login shell.
#[tauri::command]
pub async fn setup_view(office: OfficeState<'_>) -> Result<SetupView, String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.setup_view())
        .await
        .map_err(|e| format!("the setup did not load: {e}"))
}

#[tauri::command]
pub async fn import_setup_item(
    office: OfficeState<'_>,
    from: ToolKind,
    kind: ItemKind,
    name: String,
) -> Result<SetupView, String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.import_setup_item(from, kind, &name))
        .await
        .map_err(|e| format!("the copy did not finish: {e}"))?
}

#[tauri::command]
pub async fn set_setup_targets(
    office: OfficeState<'_>,
    kind: ItemKind,
    name: String,
    targets: Targets,
) -> Result<SetupView, String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.set_setup_targets(kind, &name, targets))
        .await
        .map_err(|e| format!("the setup did not save: {e}"))?
}

#[tauri::command]
pub fn structured_backlog(office: OfficeState<'_>, session_id: String) -> Result<Backlog, String> {
    office.structured_backlog(&session_id)
}

#[tauri::command]
pub fn structured_prompt(
    office: OfficeState<'_>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    office.structured_prompt(&session_id, &text)
}

#[tauri::command]
pub fn structured_answer(
    office: OfficeState<'_>,
    session_id: String,
    approval_id: String,
    choice_id: String,
    values: crate::structured::Values,
) -> Result<(), String> {
    office.structured_answer(&session_id, &approval_id, &choice_id, &values)
}

#[tauri::command]
pub fn structured_cancel(office: OfficeState<'_>, session_id: String) -> Result<(), String> {
    office.structured_cancel(&session_id)
}

#[tauri::command]
pub fn structured_stop(office: OfficeState<'_>, session_id: String) -> Result<(), String> {
    office.structured_stop(&session_id)
}

/// On a blocking thread: it waits for the agent to start, to catch one that fails.
#[tauri::command]
pub async fn spawn_session(
    office: OfficeState<'_>,
    project_id: String,
    request: HireRequest,
) -> Result<Session, String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.spawn_session(&project_id, &request))
        .await
        .map_err(|e| format!("the hire did not finish: {e}"))?
}

/// On a blocking thread, like a hire: it waits for the agent to start.
#[tauri::command]
pub async fn resume_session(office: OfficeState<'_>, session_id: String) -> Result<(), String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || office.resume_session(&session_id))
        .await
        .map_err(|e| format!("the resume did not finish: {e}"))?
}

#[tauri::command]
pub fn answer_question(
    office: OfficeState<'_>,
    question_id: String,
    answer: QuestionAnswer,
) -> Result<(), String> {
    office.answer_question(&question_id, &answer)
}

/// On a blocking thread: ticking a connector's item waits for its service to answer.
#[tauri::command]
pub async fn tick_todo(
    app: AppHandle,
    office: OfficeState<'_>,
    todo_id: String,
) -> Result<(), String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || {
        office.tick_todo(&todo_id)?;
        push_snapshot(&app, &office);
        Ok(())
    })
    .await
    .map_err(|e| format!("the tick did not finish: {e}"))?
}

#[tauri::command]
pub fn send_agent_message(
    office: OfficeState<'_>,
    from_session_id: String,
    to_session_id: String,
    text: String,
) -> Result<(), String> {
    office.send_agent_message(&from_session_id, &to_session_id, &text)
}

#[tauri::command]
pub fn focus_session(office: OfficeState<'_>, session_id: String) -> Result<(), String> {
    office.focus_session(&session_id)
}

/// Stopping the floor's agents waits for them to exit, so it runs off the main thread.
#[tauri::command]
pub async fn demolish_floor(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
) -> Result<(), String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || {
        office.demolish_floor(&project_id)?;
        push_snapshot(&app, &office);
        Ok(())
    })
    .await
    .map_err(|e| format!("the demolition did not finish: {e}"))?
}

#[tauri::command]
pub fn add_floor(app: AppHandle, office: OfficeState<'_>, path: String) -> Result<Project, String> {
    let project = office.add_floor(&path)?;
    push_snapshot(&app, &office);
    Ok(project)
}

/// Open a plain login shell in the floor's folder (Cmd+N).
#[tauri::command]
pub fn open_shell(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
) -> Result<Shell, String> {
    let shell = office.open_shell(&project_id)?;
    push_snapshot(&app, &office);
    Ok(shell)
}

/// Hang up a plain shell. The snapshot drops it when its terminal exits.
#[tauri::command]
pub fn end_shell(office: OfficeState<'_>, shell_id: String) -> Result<(), String> {
    office.end_shell(&shell_id)
}

/// Runs the native picker off the main thread (it blocks until the human chooses).
#[tauri::command]
pub async fn pick_folder(office: OfficeState<'_>) -> Result<Option<String>, String> {
    office.pick_folder()
}

/// The kept output of a session's in-app terminal; live chunks follow as events.
#[tauri::command]
pub fn terminal_backlog(
    office: OfficeState<'_>,
    session_id: String,
) -> Result<TerminalBacklog, String> {
    office.terminal_backlog(&session_id)
}

#[tauri::command]
pub fn terminal_write(
    office: OfficeState<'_>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    office.terminal_write(&session_id, &data)
}

#[tauri::command]
pub fn terminal_resize(
    office: OfficeState<'_>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    office.terminal_resize(&session_id, cols, rows)
}

#[tauri::command]
#[must_use]
pub fn get_settings(settings: State<'_, Arc<SettingsStore>>) -> Settings {
    settings.get()
}

/// A changed agent label re-sorts the connectors' items, so it polls them again. Changed QA
/// settings ask GitHub again at once.
#[tauri::command]
pub fn update_settings(
    settings: State<'_, Arc<SettingsStore>>,
    hub: HubState<'_>,
    watch: WatchState<'_>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let connectors_changed = patch.connectors.is_some();
    let qa_changed = patch.qa.is_some();
    let updated = settings.update(patch)?;
    if connectors_changed {
        hub.wake();
    }
    if qa_changed {
        watch.recheck();
    }
    Ok(updated)
}

/// The desk whose panel is open (None: none), so it does not alert while the window is focused.
#[tauri::command]
pub fn set_open_desk(alerts: State<'_, Arc<Alerts>>, session_id: Option<String>) {
    alerts.set_open_desk(session_id);
}

/// A test alert in three seconds, by the methods switched on.
#[tauri::command]
pub fn test_alert(app: AppHandle, alerts: State<'_, Arc<Alerts>>) -> Result<(), String> {
    alerts.test(app)
}

/// On a blocking thread: finding the editors runs a login shell.
#[tauri::command]
pub async fn editors(
    editors: State<'_, Arc<Editors>>,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<EditorList, String> {
    let editors = Arc::clone(&editors);
    let preferred = settings.get().editor.preferred;
    tauri::async_runtime::spawn_blocking(move || editors.list(preferred))
        .await
        .map_err(|e| format!("the editors could not be listed: {e}"))
}

/// On a blocking thread: it waits for the editor's command to hand over.
#[tauri::command]
pub async fn open_in_editor(
    office: OfficeState<'_>,
    editors: State<'_, Arc<Editors>>,
    settings: State<'_, Arc<SettingsStore>>,
    place: Place,
    editor: Option<Editor>,
) -> Result<Editor, String> {
    let folder = office.folder_of(&place)?;
    let editors = Arc::clone(&editors);
    let preferred = settings.get().editor.preferred;
    tauri::async_runtime::spawn_blocking(move || editors.open(&folder, editor, preferred))
        .await
        .map_err(|e| format!("the editor did not open: {e}"))?
}

/// On a blocking thread: it may read the Keychain, which can wait on a system prompt.
#[tauri::command]
pub async fn linear_key_status(linear: LinearState<'_>) -> Result<LinearKeyStatus, String> {
    let linear = Arc::clone(&linear);
    tauri::async_runtime::spawn_blocking(move || linear.key_status())
        .await
        .map_err(|e| format!("the key's status could not be read: {e}"))
}

/// On a blocking thread: the key is checked with Linear, then written to the Keychain.
/// The only command that ever receives the key. None removes the saved one.
#[tauri::command]
pub async fn set_linear_key(
    hub: HubState<'_>,
    linear: LinearState<'_>,
    key: Option<String>,
) -> Result<LinearKeyStatus, String> {
    let (hub, linear) = (Arc::clone(&hub), Arc::clone(&linear));
    tauri::async_runtime::spawn_blocking(move || {
        let status = linear.set_key(key)?;
        hub.recheck();
        Ok(status)
    })
    .await
    .map_err(|e| format!("the key could not be saved: {e}"))?
}

/// On a blocking thread: it asks Linear.
#[tauri::command]
pub async fn linear_teams(linear: LinearState<'_>) -> Result<Vec<LinearTeam>, String> {
    let linear = Arc::clone(&linear);
    tauri::async_runtime::spawn_blocking(move || linear.teams())
        .await
        .map_err(|e| format!("the Linear teams could not be listed: {e}"))?
}

#[tauri::command]
pub fn connect_floor(
    app: AppHandle,
    office: OfficeState<'_>,
    hub: HubState<'_>,
    project_id: String,
    link: Link,
) -> Result<(), String> {
    office.connect_floor(&project_id, link)?;
    hub.wake();
    push_snapshot(&app, &office);
    Ok(())
}

#[tauri::command]
pub fn disconnect_floor(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
    connector: ConnectorKind,
) -> Result<(), String> {
    office.disconnect_floor(&project_id, connector)?;
    push_snapshot(&app, &office);
    Ok(())
}

/// On a blocking thread: a connector's state lock can be held across a Keychain prompt.
#[tauri::command]
pub async fn recheck_connectors(hub: HubState<'_>) -> Result<(), String> {
    let hub = Arc::clone(&hub);
    tauri::async_runtime::spawn_blocking(move || hub.recheck())
        .await
        .map_err(|e| format!("the re-check did not start: {e}"))
}

/// The URL comes from the hub's cache, never from the webview, and must be the
/// connector's own site. On a blocking thread: it waits for `open` to hand over.
#[tauri::command]
pub async fn open_todo_link(office: OfficeState<'_>, todo_id: String) -> Result<(), String> {
    let url = office.todo_link(&todo_id)?;
    let (_, kind, _) =
        connectors::parse_item_id(&todo_id).ok_or("that item has no page to open")?;
    let call = connectors::link_call(kind, &url)?;
    tauri::async_runtime::spawn_blocking(move || match SystemRunner.launch(&call, OPEN_WAIT)? {
        Some(0) | None => Ok(()),
        Some(_) => Err("the link did not open".to_string()),
    })
    .await
    .map_err(|e| format!("the link did not open: {e}"))?
}

/// Switch a floor's QA desk on or off. Switching it on pins the floor.
#[tauri::command]
pub fn set_floor_qa(
    app: AppHandle,
    office: OfficeState<'_>,
    watch: WatchState<'_>,
    project_id: String,
    enabled: bool,
) -> Result<(), String> {
    office.set_floor_qa(&project_id, enabled)?;
    push_snapshot(&app, &office);
    watch.recheck();
    Ok(())
}

/// On a blocking thread, like a hire: it waits for the manager to start.
#[tauri::command]
pub async fn hire_manager(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
) -> Result<(), String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || {
        office.hire_manager(&project_id)?;
        push_snapshot(&app, &office);
        Ok(())
    })
    .await
    .map_err(|e| format!("the hire did not finish: {e}"))?
}

/// On a blocking thread: resuming hands over every note that waited, with a pause between each.
#[tauri::command]
pub async fn set_manager_paused(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
    paused: bool,
) -> Result<(), String> {
    let office = Arc::clone(&office);
    tauri::async_runtime::spawn_blocking(move || {
        office.set_manager_paused(&project_id, paused)?;
        push_snapshot(&app, &office);
        Ok(())
    })
    .await
    .map_err(|e| format!("the change did not finish: {e}"))?
}

#[tauri::command]
pub fn set_manager_hiring(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
    allowed: bool,
) -> Result<(), String> {
    office.set_manager_hiring(&project_id, allowed)?;
    push_snapshot(&app, &office);
    Ok(())
}

#[tauri::command]
pub fn withdraw_manager_note(
    app: AppHandle,
    office: OfficeState<'_>,
    project_id: String,
    note_id: String,
) -> Result<(), String> {
    office.withdraw_manager_note(&project_id, &note_id)?;
    push_snapshot(&app, &office);
    Ok(())
}

/// Ask GitHub for pull requests again now. It only sets a flag and wakes the watch.
#[tauri::command]
pub fn recheck_qa(watch: WatchState<'_>) {
    watch.recheck();
}

/// The URL is built from the stored review, never taken from the webview, and must be a
/// GitHub page. On a blocking thread: it waits for `open` to hand over.
#[tauri::command]
pub async fn open_review(
    office: OfficeState<'_>,
    project_id: String,
    number: u64,
    head: String,
) -> Result<(), String> {
    let url = office.review_link(&project_id, number, &head)?;
    let call = browser_call(&url, "https://github.com/")
        .ok_or_else(|| "that link does not go to GitHub".to_string())?;
    tauri::async_runtime::spawn_blocking(move || match SystemRunner.launch(&call, OPEN_WAIT)? {
        Some(0) | None => Ok(()),
        Some(_) => Err("the link did not open".to_string()),
    })
    .await
    .map_err(|e| format!("the link did not open: {e}"))?
}
