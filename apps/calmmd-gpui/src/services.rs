//! GPUI adapters around `calmmd-core` services.

use calmmd_core::{
    core::{
        autosave::{create_app_autosave, AppAutosave},
        history::{app_history, AppHistory, AppHistoryStore},
        session::{app_store, SessionStore},
        workspace::WorkspaceRegistry,
    },
    windows::SpaceClaims,
    AppContext, EventSink, WindowInfo, WindowRegistry,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// One core-originated event, delivered to GPUI views.
#[derive(Clone, Debug)]
pub struct CoreEvent {
    pub target: Option<String>,
    pub name: String,
    pub payload: Value,
}

/// Event sink that forwards core events into a channel consumed by GPUI.
pub struct ChannelEvents {
    tx: mpsc::UnboundedSender<CoreEvent>,
}

impl EventSink for ChannelEvents {
    fn emit(&self, target: Option<&str>, name: &str, payload: Value) {
        let _ = self.tx.send(CoreEvent {
            target: target.map(str::to_owned),
            name: name.to_owned(),
            payload,
        });
    }
}

/// Window registry shared by the shell and MCP.
#[derive(Default)]
pub struct ShellWindows {
    inner: Mutex<Vec<WindowInfo>>,
}

impl ShellWindows {
    pub fn register(&self, label: &str) {
        let mut windows = self.inner.lock().unwrap();
        if !windows.iter().any(|window| window.label == label) {
            windows.push(WindowInfo {
                label: label.to_owned(),
                title: "CalmMD".into(),
                space_id: None,
            });
        }
    }

    #[allow(dead_code)]
    pub fn remove(&self, label: &str) {
        self.inner.lock().unwrap().retain(|window| window.label != label);
    }

    #[allow(dead_code)]
    pub fn set_space(&self, label: &str, space: Option<&str>) {
        if let Some(window) = self
            .inner
            .lock()
            .unwrap()
            .iter_mut()
            .find(|window| window.label == label)
        {
            window.space_id = space.map(str::to_owned);
        }
    }
}

impl WindowRegistry for ShellWindows {
    fn windows(&self) -> Vec<WindowInfo> {
        self.inner.lock().unwrap().clone()
    }

    fn space_holder(&self, space: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .find(|window| window.space_id.as_deref() == Some(space))
            .map(|window| window.label.clone())
    }
}

/// All core services shared by windows in one GPUI process.
#[allow(dead_code)]
pub struct Services {
    pub context: AppContext,
    pub events: Option<mpsc::UnboundedReceiver<CoreEvent>>,
    pub windows: Arc<ShellWindows>,
    pub claims: Arc<SpaceClaims>,
    pub workspace: Arc<WorkspaceRegistry>,
    pub autosave: Arc<AppAutosave>,
    pub session: SessionStore<AppHistoryStore>,
    pub history: AppHistory,
}

impl Services {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let windows = Arc::new(ShellWindows::default());
        windows.register("main");
        let claims = Arc::new(SpaceClaims::default());
        let context = AppContext::new(
            data_dir,
            Arc::new(ChannelEvents { tx }),
            windows.clone() as Arc<dyn WindowRegistry>,
        );

        Self {
            workspace: WorkspaceRegistry::new(context.clone()),
            autosave: Arc::new(create_app_autosave(&context)),
            session: app_store(&context),
            history: app_history(&context),
            context,
            events: Some(events),
            windows,
            claims,
        }
    }
}
