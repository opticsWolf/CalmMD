//! GPUI view state backed by `calmmd-core` services.

use crate::services::{CoreEvent, Services};
use calmmd_core::{
    AppContext,
    core::{
        autosave::{AutosaveRequest, ResolveChoice},
        dto::WorkspaceDocumentDto,
        links,
        spell,
    },
};
use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, SharedString, Subscription, Window};
use serde_json::Value;
use std::{collections::VecDeque, path::Path as StdPath, sync::Arc};

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

#[derive(Clone, Debug)]
pub struct SearchResultItem {
    pub path: String,
    pub line: usize,
    pub text: String,
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
    search_query: String,
    search_results: Vec<SearchResultItem>,
    spell_language: Option<String>,
    autosave_status: String,
    conflict: bool,
    dirty: bool,
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
            search_query: String::new(),
            search_results: Vec::new(),
            spell_language: None,
            autosave_status: "clean".into(),
            conflict: false,
            dirty: false,
            _event_subscription: None,
        };
        if let Ok(status) = this.services.workspace.status(
            &this.window_label,
            &this.window_label,
            Value::Array(Vec::new()),
        ) {
            this.root = status.get("root").and_then(Value::as_str).map(str::to_owned);
            this.active = status
                .get("activeDocument")
                .and_then(Value::as_str)
                .map(str::to_owned);
            this.refresh_documents();
            if let Some(active) = this.active.clone() {
                this.open_document(&active, cx);
            }
        }
        this._event_subscription = Some(cx.subscribe(&this.events, |this, _, event: &CoreEvent, cx| {
            if event
                .target
                .as_deref()
                .map(|target| target == this.window_label)
                .unwrap_or(true)
            {
                match event.name.as_str() {
                    "calmmd://workspace" => this.apply_workspace_event(event),
                    "calmmd://autosave" => this.apply_autosave_event(event),
                    "calmmd://open-paths" => this.open_dropped_paths(
                        event.payload.as_array().cloned().unwrap_or_default(),
                        cx,
                    ),
                    _ => return,
                }
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

    pub fn context(&self) -> &AppContext {
        &self.services.context
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

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn search_results(&self) -> &[SearchResultItem] {
        &self.search_results
    }

    pub fn spell_language(&self) -> Option<&str> {
        self.spell_language.as_deref()
    }

    pub fn autosave_status(&self) -> &str {
        &self.autosave_status
    }

    pub fn conflict(&self) -> bool {
        self.conflict
    }

    pub fn dirty(&self) -> bool {
        self.dirty
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
        self.search_results.clear();
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
                self.autosave_status = "clean".into();
                self.conflict = false;
                self.dirty = false;
                self.message = format!("Opened {}", self.active.as_deref().unwrap_or(path));
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    /// Opens paths dropped onto the window or forwarded by another launch.
    pub fn open_dropped_paths(&mut self, paths: Vec<Value>, cx: &mut Context<Self>) {
        let mut folders = Vec::new();
        let mut documents = Vec::new();
        for value in paths {
            let Some(raw) = value.as_str() else { continue };
            let path = StdPath::new(raw);
            if path.is_dir() {
                folders.push(path.to_owned());
            } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")) {
                documents.push(raw.to_owned());
            }
        }
        if let Some(folder) = folders.into_iter().next() {
            self.open_folder(folder, cx);
        }
        if self.root.is_none() {
            if let Some(document) = documents.first() {
                if let Some(parent) = StdPath::new(document).parent() {
                    self.open_folder(parent.to_owned(), cx);
                }
            }
        }
        for document in documents {
            self.open_document(&document, cx);
        }
    }

    /// Opens a link from the preview using the same resolver as the Rust core.
    pub fn open_link(&mut self, url: &str, cx: &mut Context<Self>) {
        if let Some(fragment) = url.strip_prefix('#') {
            self.message = format!("Anchor: #{fragment}");
            cx.notify();
            return;
        }
        let Some(dir) = self.active.as_deref().map(parent_dir) else {
            self.message = "Open a document before following a link".into();
            cx.notify();
            return;
        };
        let Some(path) = links::resolve_link(url, &dir) else {
            self.message = format!("External link: {url}");
            cx.notify();
            return;
        };
        self.open_document(&path, cx);
    }

    /// Opens a `[[wiki link]]` by path, relative path or note stem.
    pub fn open_wiki_link(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        let cleaned = target.split('|').next().unwrap_or(target).trim();
        let direct = if cleaned.to_ascii_lowercase().ends_with(".md") {
            cleaned.to_owned()
        } else {
            format!("{cleaned}.md")
        };
        if self.documents.iter().any(|doc| doc.path == direct) {
            self.open_document(&direct, cx);
            return;
        }
        let stem = cleaned.to_ascii_lowercase();
        if let Some(doc) = self.documents.iter().find(|doc| {
            let name = doc.path.rsplit(['/', '\\']).next().unwrap_or(&doc.path).to_ascii_lowercase();
            name == stem || name.trim_end_matches(".md") == stem
        }) {
            let path = doc.path.clone();
            self.open_document(&path, cx);
            return;
        }
        let _ = window;
        self.message = format!("Linked note not found: {cleaned}");
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

    /// Called by the editor on every user edit. Rust owns the autosave timer.
    pub fn editor_changed(&mut self, text: String, cx: &mut Context<Self>) {
        if self.text == text {
            return;
        }
        self.text = text.clone();
        self.dirty = true;
        if let Some(path) = self.active.clone() {
            let request = AutosaveRequest {
                buffer: calmmd_core::core::buffers::PersistRequest {
                    id: path.clone(),
                    owner: Some(self.window_label.clone()),
                    untitled_index: 0,
                    path: Some(path),
                    text,
                    eol: "\n".into(),
                    bom: false,
                    mtime_ms: None,
                    conflict: false,
                },
                idle_ms: 1000,
                max_wait_ms: 10_000,
                retry_delays_ms: vec![1000, 3000, 10_000],
            };
            self.services.autosave.touch(request);
            self.autosave_status = "dirty".into();
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
            Ok(_) => {
                let _ = self.services.history.save(&path, &self.text, Some("GPUI save"));
                self.services.autosave.dispose(&path);
                self.dirty = false;
                self.autosave_status = "clean".into();
                self.message = "Saved".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn undo_active(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else { return };
        match self.services.workspace.undo_document(&self.window_label, &path) {
            Ok(value) => {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    self.text = text.to_owned();
                    self.dirty = true;
                }
                self.message = "Undo".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn redo_active(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else { return };
        match self.services.workspace.redo_document(&self.window_label, &path) {
            Ok(value) => {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    self.text = text.to_owned();
                    self.dirty = true;
                }
                self.message = "Redo".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn resolve_conflict(&mut self, keep_mine: bool, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else { return };
        let choice = if keep_mine {
            ResolveChoice::KeepMine
        } else {
            ResolveChoice::LoadTheirs
        };
        match self.services.autosave.resolve(&path, choice) {
            Ok(result) => {
                if let Some(file) = result.file {
                    self.text = file.text;
                    self.dirty = false;
                } else if keep_mine {
                    self.dirty = false;
                }
                self.autosave_status = result.state.status;
                self.conflict = result.state.conflict;
                self.message = if keep_mine { "Kept local changes" } else { "Loaded disk version" }.into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_query = query;
        self.run_search();
        cx.notify();
    }

    fn run_search(&mut self) {
        if self.search_query.trim().is_empty() {
            self.search_results.clear();
            return;
        }
        self.search_results = self
            .services
            .workspace
            .search(&self.window_label, &self.search_query, false, false)
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<Value>>(value.get("hits")?.clone()).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|hit| {
                Some(SearchResultItem {
                    path: hit.get("path")?.as_str()?.to_owned(),
                    line: hit.get("line")?.as_u64().unwrap_or(0) as usize,
                    text: hit.get("text")?.as_str()?.to_owned(),
                })
            })
            .collect();
    }

    pub fn cycle_spell_language(&mut self, cx: &mut Context<Self>) {
        let available: Vec<(String, String)> = spell::dictionaries(&self.services.context)
            .unwrap_or_default()
            .into_iter()
            .filter(|dict| dict.installed || dict.bundled)
            .filter_map(|dict| {
                let tag = dict.languages.first()?.clone();
                Some((dict.id, tag))
            })
            .collect();
        if available.is_empty() {
            self.message = "No dictionary available".into();
            cx.notify();
            return;
        }
        let current_id = self
            .spell_language
            .as_deref()
            .and_then(|tag| spell::entry_for_lang(Some(&tag)))
            .map(|entry| entry.id);
        let next = available
            .iter()
            .position(|(id, _)| Some(id.as_str()) == current_id)
            .map(|index| (index + 1) % available.len())
            .unwrap_or(0);
        self.spell_language = Some(available[next].1.clone());
        self.message = format!("Spell language: {}", available[next].0);
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
                        self.dirty = false;
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

    fn apply_autosave_event(&mut self, event: &CoreEvent) {
        let Some(state) = event.payload.get("state") else { return };
        let id = state.get("id").and_then(Value::as_str);
        if id != self.active.as_deref() {
            return;
        }
        self.autosave_status = state
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        self.conflict = state.get("conflict").and_then(Value::as_bool).unwrap_or(false);
        self.dirty = state.get("dirty").and_then(Value::as_bool).unwrap_or(self.dirty);
        if self.conflict {
            self.message = "Save conflict — choose a version".into();
        } else if self.autosave_status == "clean" {
            self.message = "Autosaved".into();
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
            dirty: self.dirty,
            conflict: self.conflict,
        })
    }

    pub fn title(&self) -> SharedString {
        match self.active.as_deref() {
            Some(path) => path.rsplit(['/', '\\']).next().unwrap_or(path).into(),
            None => "CalmMD".into(),
        }
    }
}

fn parent_dir(path: &str) -> String {
    StdPath::new(path)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}
