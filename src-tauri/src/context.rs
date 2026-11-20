//! Adapters that let Tauri host the UI-independent `calmmd-core` services.

use calmmd_core::{windows::SpaceClaims, AppContext, EventSink, WindowInfo, WindowRegistry};
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

struct TauriEvents {
    app: AppHandle,
}

impl EventSink for TauriEvents {
    fn emit(&self, target: Option<&str>, name: &str, payload: Value) {
        match target {
            Some(label) => {
                let _ = self.app.emit_to(label, name, payload);
            }
            None => {
                let _ = self.app.emit(name, payload);
            }
        }
    }
}

struct TauriWindows {
    app: AppHandle,
    claims: Arc<SpaceClaims>,
}

impl WindowRegistry for TauriWindows {
    fn windows(&self) -> Vec<WindowInfo> {
        self.app
            .webview_windows()
            .into_values()
            .map(|window| {
                let label = window.label().to_string();
                let title = window.title().unwrap_or_else(|_| label.clone());
                WindowInfo {
                    label,
                    title,
                    space_id: self.claims.space_for_window(&window_label_owned(&window)),
                }
            })
            .collect()
    }

    fn space_holder(&self, space: &str) -> Option<String> {
        self.claims.holder(space)
    }
}

fn window_label_owned(window: &tauri::WebviewWindow) -> String {
    window.label().to_string()
}

/// State shared by Tauri commands. Core services receive only the framework-free context.
#[derive(Clone)]
pub struct AppState {
    pub context: AppContext,
    pub claims: Arc<SpaceClaims>,
}

pub fn app_context(app: &AppHandle, claims: Arc<SpaceClaims>) -> AppContext {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("CalmMD app data directory");
    AppContext::new(
        data_dir,
        Arc::new(TauriEvents { app: app.clone() }),
        Arc::new(TauriWindows {
            app: app.clone(),
            claims,
        }),
    )
}
