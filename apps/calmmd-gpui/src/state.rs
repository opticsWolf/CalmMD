//! GPUI view state backed by `calmmd-core` services.

use crate::services::{CoreEvent, Services};
use calmmd_core::core::dto::WorkspaceDocumentDto;
use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, SharedString, Subscription};
use serde_json::Value;
use std::{collections::VecDeque, sync::Arc};

/// Queues core events for GPUI views. The channel is drained by a GPUI task.
pub struct CoreEvents {
    recent: VecDeque<CoreEvent>,
    capacity: usize,
}

impl EventEmitter<CoreEvent> for CoreEvents {}

impl CoreEvents {
    pub fn new(mut rx: tokio::sync::mpsc::UnboundedReceiver<CoreEvent>, cx: &mut App) -> Entity<Self> {
        let this = cx.new(|_| Self {
            recent: VecDeque::new(),
            capacity: 64,
        });
        let weak = this.downgrade();
        cx.spawn(async move |cx| {
            while let Some(event) = rx.recv().await {
                let _ = weak.update(cx, |this, cx| {
                    this.recent.push_back(event.clone());
                    while this.recent.len() > this.capacity {
                        this.recent.pop_front();
                    }
                    cx.emit(event);
                });
            }
        })
        .detach();
        this
    }

    pub fn recent(&self) -> &VecDeque<CoreEvent> {
        &self.recent
    }
}

/// A document listed in the sidebar.
#[derive(Clone, Debug)]
pub struct DocumentItem {
    pub path: String,
    pub name: String,
}

/// Window-local state. All durable state is delegated to core services.
pub struct WorkspaceState {
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    window_label: String,
    root: Option<String>,
    documents: Vec<DocumentItem>,
    active: Option<String>,
    text: String,
    message: String,
    _event_subscription: Option<Subscription>,
}

#[allow(dead_code)]
impl WorkspaceState {
    pub fn new(
        services: Arc<Services>,
        events: Entity<CoreEvents>,
        window_label: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            services,
            events,
            window_label,
            root: None,
            documents: Vec::new(),
            active: None,
            text: "# Welcome to CalmMD\n\nOpen a folder to begin.".into(),
            message: "Ready".into(),
            _event_subscription: None,
        };
        this._event_subscription = Some(cx.subscribe(&this.events, |this, _, event: &CoreEvent, cx| {
            if event
                .target
                .as_deref()
                .map(|target| target == this.window_label)
                .unwrap_or(true)
                && event.name == "calmmd://workspace"
            {
                this.apply_workspace_event(event);
                cx.notify();
            }
        }));
        this
    }

    pub fn services(&self) -> &Arc<Services> {
        &self.services
    }

    pub fn events(&self) -> &Entity<CoreEvents> {
        &self.events
    }

    pub fn window_label(&self) -> &str {
        &self.window_label
    }

    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    pub fn documents(&self) -> &[DocumentItem] {
        &self.documents
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn open_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let path = path.to_string_lossy().into_owned();
        let result = self.services.workspace.sync(
            &self.window_label,
            calmmd_core::core::dto::WorkspaceSnapshotDto {
                root: Some(path.clone()),
                documents: Vec::new(),
                active_document: None,
            },
        );
        if let Err(error) = result {
            self.message = error;
            cx.notify();
            return;
        }
        self.root = Some(path);
        self.refresh_documents();
        self.active = None;
        self.text = "Select a document, or create a new one.".into();
        self.message = "Folder opened".into();
        cx.notify();
    }

    pub fn open_document(&mut self, path: &str, cx: &mut Context<Self>) {
        match self.services.workspace.read_document(&self.window_label, path) {
            Ok(value) => {
                self.active = value.get("path").and_then(Value::as_str).map(str::to_owned);
                self.text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.message = format!("Opened {}", self.active.as_deref().unwrap_or(path));
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn create_document(&mut self, name: &str, cx: &mut Context<Self>) {
        let result = self.services.workspace.create_document(
            &self.window_label,
            None,
            name,
            Some("# New note\n"),
        );
        match result {
            Ok(value) => {
                let path = value.get("path").and_then(Value::as_str).unwrap_or(name).to_owned();
                self.refresh_documents();
                self.open_document(&path, cx);
                self.message = "Document created".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn save_active(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else {
            self.message = "No document is open".into();
            cx.notify();
            return;
        };
        match self.services.workspace.write_document(
            &self.window_label,
            &path,
            &self.text,
            "replace",
            true,
        ) {
            Ok(_) => self.message = "Saved".into(),
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn claim_space(&mut self, space: Option<&str>, cx: &mut Context<Self>) {
        if let Some(other) = self.services.claims.claim(&self.window_label, space) {
            self.message = format!("Space is open in {other}");
        } else {
            self.services.windows.set_space(&self.window_label, space);
            self.message = match space {
                Some(space) => format!("Space set to {space}"),
                None => "Space cleared".into(),
            };
        }
        cx.notify();
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.text = text;
        cx.notify();
    }

    fn refresh_documents(&mut self) {
        self.documents = self
            .services
            .workspace
            .list_documents(&self.window_label, true, None)
            .ok()
            .and_then(|value| {
                serde_json::from_value::<Vec<Value>>(value.get("documents")?.clone()).ok()
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| {
                Some(DocumentItem {
                    path: value.get("path")?.as_str()?.to_owned(),
                    name: value.get("name")?.as_str()?.to_owned(),
                })
            })
            .collect();
    }

    fn apply_workspace_event(&mut self, event: &CoreEvent) {
        let kind = event.payload.get("kind").and_then(Value::as_str).unwrap_or("");
        let path = event.payload.get("path").and_then(Value::as_str);
        match kind {
            "content" => {
                if let Some(body) = event.payload.get("text").and_then(Value::as_str) {
                    if path == self.active.as_deref() {
                        self.text = body.to_owned();
                    }
                }
                self.message = "Document changed in Rust".into();
            }
            "created" | "renamed" | "moved" | "trashed" => {
                self.refresh_documents();
                self.message = format!("Workspace changed: {kind}");
            }
            _ => {}
        }
    }

    /// Bridges one live Rust document into the webview-compatible DTO used by MCP.
    pub fn active_dto(&self) -> Option<WorkspaceDocumentDto> {
        let path = self.active.as_deref()?;
        Some(WorkspaceDocumentDto {
            path: path.to_owned(),
            text: self.text.clone(),
            eol: "\n".into(),
            bom: false,
            mtime_ms: None,
            dirty: false,
            conflict: false,
        })
    }

    pub fn title(&self) -> SharedString {
        match self.active.as_deref() {
            Some(path) => path.rsplit(['/', '\\']).next().unwrap_or(path).into(),
            None => "CalmMD".into(),
        }
    }
}
