//! Tauri commands for the core key/value store.

use crate::context::AppState;
use calmmd_core::kv as core;
use tauri::State;

#[tauri::command]
pub async fn kv_get(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    core::read(&state.context, &key)
}

#[tauri::command]
pub async fn kv_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    core::write(&state.context, &key, &value)
}

#[tauri::command]
pub async fn kv_delete(state: State<'_, AppState>, key: String) -> Result<(), String> {
    core::delete(&state.context, &key)
}

#[tauri::command]
pub async fn kv_list(state: State<'_, AppState>, prefix: String) -> Result<Vec<String>, String> {
    core::list(&state.context, &prefix)
}
