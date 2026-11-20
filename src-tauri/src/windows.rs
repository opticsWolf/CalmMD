//! Tauri window commands. Claim bookkeeping lives in `calmmd-core`.

use crate::context::AppState;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, Window};

#[tauri::command]
pub fn claim_space(window: Window, state: State<'_, AppState>, space_id: Option<String>) -> Option<String> {
    state.claims.claim(window.label(), space_id.as_deref())
}

#[tauri::command]
pub fn space_window(state: State<'_, AppState>, space_id: String) -> Option<String> {
    state.claims.holder(&space_id)
}

#[tauri::command]
pub async fn open_window(
    app: AppHandle,
    state: State<'_, AppState>,
    space_id: Option<String>,
) -> Result<String, String> {
    let label = loop {
        let candidate = state.claims.next_label();
        if app.get_webview_window(&candidate).is_none() {
            break candidate;
        }
    };
    let init = format!(
        "window.__CALMMD_SPACE__ = {};",
        serde_json::to_string(&space_id).map_err(|e| e.to_string())?
    );
    let offset = 28.0 * (app.webview_windows().len() as f64 % 8.0);
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("CalmMD")
        .inner_size(1100.0, 760.0)
        .min_inner_size(480.0, 360.0)
        .position(80.0 + offset, 60.0 + offset)
        .initialization_script(&init)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(label)
}

#[tauri::command]
pub fn focus_window(app: AppHandle, label: String) -> Result<(), String> {
    let window = app.get_webview_window(&label).ok_or("window not found")?;
    let _ = window.unminimize();
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_labels(app: AppHandle) -> Vec<String> {
    app.webview_windows().keys().cloned().collect()
}
