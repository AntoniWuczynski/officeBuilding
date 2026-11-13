//! Tauri command: the Rust side of `HandoverApi` (src/api/HandoverApi.ts). The webview sends a
//! session id and nothing else.

use crate::office::Office;
use crate::watch::SNAPSHOT_EVENT;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// On a blocking thread: it waits for one process to exit and another to start. A snapshot is
/// pushed afterwards whether it worked or not.
#[tauri::command]
pub async fn hand_over_desk(
    app: AppHandle,
    office: State<'_, Arc<Office>>,
    session_id: String,
) -> Result<(), String> {
    let office = Arc::clone(&office);
    let working = Arc::clone(&office);
    let result = tauri::async_runtime::spawn_blocking(move || working.hand_over_desk(&session_id))
        .await
        .map_err(|e| format!("the handover did not run: {e}"));
    // A closed webview just means nobody is listening right now.
    let _ = app.emit(SNAPSHOT_EVENT, office.snapshot());
    result?
}
