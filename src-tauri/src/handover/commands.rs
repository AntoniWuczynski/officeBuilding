//! Tauri commands: the Rust side of `HandoverApi` (src/api/HandoverApi.ts). The webview sends a
//! session id, and for a take-over what A Human chose for the new session.

use super::view::TakeOverStart;
use crate::office::Office;
use crate::watch::SNAPSHOT_EVENT;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Run `work` on a blocking thread, then push a snapshot whether it worked or not. `what`
/// names it in the refusal when the thread itself fails.
async fn on_blocking_thread(
    app: &AppHandle,
    office: &Arc<Office>,
    what: &str,
    work: impl FnOnce(&Office) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let working = Arc::clone(office);
    let result = tauri::async_runtime::spawn_blocking(move || work(&working))
        .await
        .map_err(|e| format!("the {what} did not run: {e}"));
    // A closed webview just means nobody is listening right now.
    let _ = app.emit(SNAPSHOT_EVENT, office.snapshot());
    result?
}

/// On a blocking thread: it waits for one process to exit and another to start.
#[tauri::command]
pub async fn hand_over_desk(
    app: AppHandle,
    office: State<'_, Arc<Office>>,
    session_id: String,
) -> Result<(), String> {
    on_blocking_thread(&app, &office, "handover", move |o| {
        o.hand_over_desk(&session_id)
    })
    .await
}

/// On a blocking thread: it reads a conversation, writes a brief and starts a tool. Nothing
/// at the old session is touched.
#[tauri::command]
pub async fn take_over_desk(
    app: AppHandle,
    office: State<'_, Arc<Office>>,
    session_id: String,
    start: TakeOverStart,
) -> Result<(), String> {
    on_blocking_thread(&app, &office, "take-over", move |o| {
        o.take_over_desk(&session_id, &start)
    })
    .await
}
