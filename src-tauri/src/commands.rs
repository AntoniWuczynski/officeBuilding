//! Tauri commands: the Rust side of `DiscoveryApi` (src/api/DiscoveryApi.ts).
//! The webview calls them with `invoke("<name>", { camelCaseArgs })`.
#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri's command macro hands each command owned, deserialized arguments and `State` by value"
)]

use crate::model::{
    HireRequest, OfficeSnapshot, Project, QuestionAnswer, Session, TerminalBacklog, ToolOptions,
};
use crate::office::Office;
use std::sync::Arc;
use tauri::State;

type OfficeState<'a> = State<'a, Arc<Office>>;

#[tauri::command]
#[must_use]
pub fn get_snapshot(office: OfficeState<'_>) -> OfficeSnapshot {
    office.snapshot()
}

#[tauri::command]
#[must_use]
pub fn hire_options(office: OfficeState<'_>) -> Vec<ToolOptions> {
    office.hire_options()
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

#[tauri::command]
pub fn add_floor(office: OfficeState<'_>, path: String) -> Result<Project, String> {
    office.add_floor(&path)
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
