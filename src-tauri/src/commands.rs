//! Tauri commands: the Rust side of `DiscoveryApi` (src/api/DiscoveryApi.ts).
//! The webview calls them with `invoke("<name>", { camelCaseArgs })`.
#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri's command macro hands each command owned, deserialized arguments and `State` by value"
)]

use crate::model::{
    HireRequest, OfficeSnapshot, Project, QuestionAnswer, Session, TerminalBacklog, ToolKind,
    ToolOptions, ToolUsage,
};
use crate::office::Office;
use crate::setup::{ItemKind, SetupView, Targets};
use crate::structured::desks::Backlog;
use crate::watch::SNAPSHOT_EVENT;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type OfficeState<'a> = State<'a, Arc<Office>>;

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

#[tauri::command]
pub fn tick_todo(office: OfficeState<'_>, todo_id: String) -> Result<(), String> {
    office.tick_todo(&todo_id)
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
