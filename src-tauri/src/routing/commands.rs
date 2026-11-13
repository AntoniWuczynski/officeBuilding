//! Tauri commands: the Rust side of `RoutingApi` (src/api/RoutingApi.ts). Thin: every decision is
//! in `keys` and `floors`.
#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri's command macro hands each command owned, deserialized arguments and `State` by value"
)]

use super::floors::{self, FloorRouting};
use super::keys::{self, KeyStatus, Keys};
use crate::office::Office;
use crate::settings::SettingsStore;
use std::sync::Arc;
use tauri::State;

type KeysState<'a> = State<'a, Arc<Keys>>;
type SettingsState<'a> = State<'a, Arc<SettingsStore>>;
type OfficeState<'a> = State<'a, Arc<Office>>;

/// On a blocking thread: it may read the Keychain, which can wait on a system prompt.
#[tauri::command]
pub async fn routing_key_status(
    keys: KeysState<'_>,
    settings: SettingsState<'_>,
    connection_id: String,
) -> Result<KeyStatus, String> {
    let (kind, id) = keys::keyed(&settings.get().routing, &connection_id)?;
    let keys = Arc::clone(&keys);
    tauri::async_runtime::spawn_blocking(move || keys.status(kind, &id))
        .await
        .map_err(|e| format!("the key's status could not be read: {e}"))
}

/// On a blocking thread: the key is written to the Keychain. The only command that ever receives
/// a key. None removes the saved one.
#[tauri::command]
pub async fn set_routing_key(
    keys: KeysState<'_>,
    settings: SettingsState<'_>,
    connection_id: String,
    key: Option<String>,
) -> Result<KeyStatus, String> {
    let (kind, id) = keys::keyed(&settings.get().routing, &connection_id)?;
    let keys = Arc::clone(&keys);
    let settings = Arc::clone(&settings);
    tauri::async_runtime::spawn_blocking(move || {
        let listed = || settings.get().routing.connection(&id).is_some();
        keys.set(kind, &id, key, listed)
    })
    .await
    .map_err(|e| format!("the key could not be saved: {e}"))?
}

#[tauri::command]
pub fn floor_routing(
    office: OfficeState<'_>,
    settings: SettingsState<'_>,
    project_id: String,
) -> Result<FloorRouting, String> {
    Ok(FloorRouting::of(
        &settings.get().routing,
        office.floor_context(&project_id)?,
    ))
}

/// Saved at once. Nothing in the snapshot changes, so none is pushed.
#[tauri::command]
pub fn set_floor_context(
    office: OfficeState<'_>,
    settings: SettingsState<'_>,
    project_id: String,
    context_id: Option<String>,
) -> Result<FloorRouting, String> {
    let routing = settings.get().routing;
    let context = context_id
        .map(|raw| floors::known_context(&routing, &raw))
        .transpose()?;
    office.set_floor_context(&project_id, context)?;
    Ok(FloorRouting::of(
        &routing,
        office.floor_context(&project_id)?,
    ))
}
