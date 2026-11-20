//! Host-independent context for Rust services.
//!
//! The Tauri and GPUI shells both construct an `AppContext`. It carries the
//! data directory, an event sink and a small window/space registry so core
//! services never need to import either UI framework.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

/// A window as seen by core services and MCP.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub label: String,
    pub title: String,
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>,
}

/// Receives events emitted by core services. The UI shell decides how to deliver them.
pub trait EventSink: Send + Sync {
    fn emit(&self, target: Option<&str>, name: &str, payload: Value);
}

/// Supplies the live window list and Space claims without exposing UI types.
pub trait WindowRegistry: Send + Sync {
    fn windows(&self) -> Vec<WindowInfo>;
    fn space_holder(&self, space: &str) -> Option<String> {
        self.windows()
            .into_iter()
            .find(|window| window.space_id.as_deref() == Some(space))
            .map(|window| window.label)
    }
}

/// A no-op event sink useful for tests and headless service setup.
#[derive(Default)]
pub struct NullEvents;

impl EventSink for NullEvents {
    fn emit(&self, _target: Option<&str>, _name: &str, _payload: Value) {}
}

/// A window registry with no windows, useful for tests.
#[derive(Default)]
pub struct EmptyWindows;

impl WindowRegistry for EmptyWindows {
    fn windows(&self) -> Vec<WindowInfo> {
        Vec::new()
    }
}

/// Everything a core service needs from its host.
#[derive(Clone)]
pub struct AppContext {
    data_dir: PathBuf,
    events: Arc<dyn EventSink>,
    windows: Arc<dyn WindowRegistry>,
}

impl AppContext {
    pub fn new(
        data_dir: PathBuf,
        events: Arc<dyn EventSink>,
        windows: Arc<dyn WindowRegistry>,
    ) -> Self {
        Self { data_dir, events, windows }
    }

    pub fn headless(data_dir: PathBuf) -> Self {
        Self::new(data_dir, Arc::new(NullEvents), Arc::new(EmptyWindows))
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn state_dir(&self) -> Result<PathBuf, String> {
        let dir = self.data_dir.join("state");
        std::fs::create_dir_all(&dir).map_err(|e| crate::fsops::describe(&e))?;
        Ok(dir)
    }

    pub fn emit_to(&self, target: &str, name: &str, payload: impl Serialize) {
        let value = serde_json::to_value(payload).unwrap_or(Value::Null);
        self.events.emit(Some(target), name, value);
    }

    pub fn emit(&self, name: &str, payload: impl Serialize) {
        let value = serde_json::to_value(payload).unwrap_or(Value::Null);
        self.events.emit(None, name, value);
    }

    pub fn windows(&self) -> Vec<WindowInfo> {
        self.windows.windows()
    }

    pub fn space_holder(&self, space: &str) -> Option<String> {
        self.windows.space_holder(space)
    }
}
