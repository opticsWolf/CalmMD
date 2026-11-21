//! GPUI view state backed by `calmmd-core` services.

use crate::services::{CoreEvent, Services};
use calmmd_core::{
    AppContext,
    core::{
        autosave::{AutosaveRequest, ResolveChoice},
        buffers::{self, DraftEntry},
        dto::WorkspaceDocumentDto,
        links,
    },
};
use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, SharedString, Subscription, Window};
use serde_json::Value;
use std::{path::Path as StdPath, sync::Arc};

/// Queues core events for GPUI views. The channel is drained by a GPUI task.
pub struct CoreEvents;

impl EventEmitter<CoreEvent> for CoreEvents {}

impl CoreEvents {
    pub fn new(mut rx: tokio::sync::mpsc::UnboundedReceiver<CoreEvent>, cx: &mut App) -> Entity<Self> {
        let this = cx.new(|_| Self);
        let weak = this.downgrade();
        cx.spawn(async move |cx| {
            while let Some(event) = rx.recv().await {
                let _ = weak.update(cx, |this, cx| {
                    let _ = this;
                    cx.emit(event);
                });
            }
        })
        .detach();
        this
    }

}

/// A document listed in the sidebar.
#[derive(Clone, Debug)]
pub struct DocumentItem {
    pub path: String,
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
    folders: Vec<String>,
    documents: Vec<DocumentItem>,
    active: Option<String>,
    tabs: Vec<String>,
    closed_tabs: Vec<String>,
    text: String,
    eol: String,
    bom: bool,
    mtime_ms: Option<f64>,
    message: String,
    search_query: String,
    search_results: Vec<SearchResultItem>,
    spell_language: Option<String>,
    autosave_status: String,
    conflict: bool,
    dirty: bool,
    recovery_drafts: Vec<DraftEntry>,
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
            folders: Vec::new(),
            documents: Vec::new(),
            active: None,
            tabs: Vec::new(),
            closed_tabs: Vec::new(),
            text: "# Welcome to CalmMD\n\nOpen a folder to begin.".into(),
            eol: "\n".into(),
            bom: false,
            mtime_ms: None,
            message: "Ready".into(),
            search_query: String::new(),
            search_results: Vec::new(),
            spell_language: default_spell_language(),
            autosave_status: "clean".into(),
            conflict: false,
            dirty: false,
            recovery_drafts: Vec::new(),
            _event_subscription: None,
        };
        if let Ok(status) = this.services.workspace.status(
            &this.window_label,
            &this.window_label,
            Value::Array(Vec::new()),
        ) {
            this.root = status.get("root").and_then(Value::as_str).map(str::to_owned);
            this.folders = status["roots"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
            this.active = status
                .get("activeDocument")
                .and_then(Value::as_str)
                .map(str::to_owned);
            this.refresh_documents();
            if let Some(active) = this.active.clone() {
                this.open_document(&active, cx);
            }
        }
        if this.window_label == "main" { this.refresh_recovery_drafts(); }
        this._event_subscription = Some(cx.subscribe(&this.events, |this, _, event: &CoreEvent, cx| {
            if event
                .target
                .as_deref()
                .map(|target| target == this.window_label)
                .unwrap_or(true)
            {
                match event.name.as_str() {
                    "calmmd://workspace" => this.apply_workspace_event(event, cx),
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

    pub fn folders(&self) -> &[String] { &self.folders }

    pub fn active_root(&self) -> Option<&str> {
        self.active.as_deref().and_then(|path| {
            self.folders.iter().filter(|root| calmmd_core::core::paths::is_within(path, root)).max_by_key(|root| root.len())
        }).map(String::as_str).or(self.root.as_deref())
    }

    pub fn favourites(&self) -> Vec<String> {
        calmmd_core::kv::read(self.context(), "gpui/favourites")
            .ok().flatten().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
    }

    fn write_favourites(&self, values: &[String]) -> Result<(), String> {
        let raw = serde_json::to_string(values).map_err(|error| error.to_string())?;
        calmmd_core::kv::write(self.context(), "gpui/favourites", &raw)
    }

    pub fn toggle_favourite(&mut self, path: &str, cx: &mut Context<Self>) {
        use calmmd_core::core::paths::path_key;
        let mut values = self.favourites();
        let key = path_key(path);
        if let Some(index) = values.iter().position(|value| path_key(value) == key) {
            values.remove(index);
        } else {
            values.push(path.to_owned());
        }
        if let Err(error) = self.write_favourites(&values) { self.message = error; }
        cx.refresh_windows();
        cx.notify();
    }

    pub fn open_favourite(&mut self, path: &str, new_tab: bool, cx: &mut Context<Self>) {
        let file = StdPath::new(path);
        let folder = if file.is_file() { file.parent() } else if file.is_dir() { Some(file) } else { None };
        let Some(folder) = folder else {
            self.set_message("This favourite is unavailable.".into(), cx);
            return;
        };
        if self.services.workspace.resolve(&self.window_label, path).is_err() {
            if let Err(error) = self.add_folder(folder.into(), cx) {
                self.set_message(error, cx);
                return;
            }
        }
        if file.is_file() { self.navigate_document(path, new_tab, cx); }
    }

    pub fn documents(&self) -> &[DocumentItem] {
        &self.documents
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn tabs(&self) -> &[String] { &self.tabs }

    pub fn close_tab(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.flush_before_close(cx) { return; }
        self.tabs.retain(|tab| tab != path);
        self.closed_tabs.push(path.to_owned());
        self.services.autosave.dispose_if_clean(&autosave_id(&self.window_label, path));
        let _ = self.services.workspace.remove_document(&self.window_label, path);
        if self.active.as_deref() == Some(path) {
            if let Some(next) = self.tabs.last().cloned() {
                self.open_document(&next, cx);
            } else {
                self.active = None;
                self.text = "Select a document, or create a new one.".into();
        self.conflict = false;
        self.dirty = false;
        self.autosave_status = "clean".into();
                self.dirty = false;
                self.conflict = false;
                self.autosave_status = "clean".into();
            }
        }
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn set_message(&mut self, message: String, cx: &mut Context<Self>) {
        self.message = message;
        cx.notify();
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

    pub fn recovery_drafts(&self) -> &[DraftEntry] { &self.recovery_drafts }

    pub fn missing_conflict_file(&self) -> bool {
        self.conflict && self.active.as_deref().is_some_and(|path| !StdPath::new(path).exists())
    }

    fn refresh_recovery_drafts(&mut self) {
        match buffers::app_store(&self.services.context).drafts() {
            Ok(drafts) => self.recovery_drafts = drafts.into_iter().filter(|draft| draft.record.path.is_some()).collect(),
            Err(error) => self.message = format!("Could not load recovery drafts: {error}"),
        }
    }

    pub fn recover_draft(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.recovery_drafts.iter().find(|draft| draft.id == id).cloned() else { return };
        let Some(path) = draft.record.path.clone() else { return };
        if !self.flush_before_close(cx) { return; }
        if !self.folders.iter().any(|root| calmmd_core::core::paths::is_within(&path, root)) {
            let Some(parent) = StdPath::new(&path).parent() else { self.message = "Draft has no parent folder".into(); cx.notify(); return };
            self.open_folder(parent.to_path_buf(), cx);
        }
        if let Err(error) = self.services.workspace.resolve(&self.window_label, &path) {
            self.message = format!("Cannot open draft: {error}"); cx.notify(); return;
        }
        let buffer_id = autosave_id(&self.window_label, &path);
        let store = buffers::app_store(&self.services.context);
        if let Err(error) = store.claim_draft(&draft, &buffer_id, &self.window_label) {
            self.message = error; cx.notify(); return;
        }
        let previous = self.active.replace(path.clone());
        if let Some(replaced) = place_tab(&mut self.tabs, previous.as_deref(), &path, false) {
            let _ = self.services.workspace.remove_document(&self.window_label, &replaced);
        }
        self.text = draft.record.text;
        self.eol = draft.record.eol;
        self.bom = draft.record.bom;
        self.mtime_ms = store.load(&path).ok().map(|file| file.mtime_ms);
        self.dirty = true;
        self.conflict = true;
        self.autosave_status = "dirty".into();
        self.services.autosave.touch(AutosaveRequest {
            buffer: buffers::PersistRequest { id: buffer_id, owner: Some(self.window_label.clone()),
                untitled_index: draft.record.untitled_index, path: Some(path), text: self.text.clone(),
                eol: self.eol.clone(), bom: self.bom, mtime_ms: self.mtime_ms, conflict: true },
            idle_ms: 3000, max_wait_ms: 30_000, retry_delays_ms: vec![3000, 10_000, 30_000],
        });
        self.sync_active();
        self.recovery_drafts.retain(|item| item.id != id);
        self.message = "Recovered draft — choose which version to keep".into();
        cx.notify();
    }

    pub fn reopen_document(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.closed_tabs.pop() {
            self.open_in_new_tab(&path, cx);
        } else {
            self.set_message("No recently closed document".into(), cx);
        }
    }

    pub fn close_folder(&mut self, cx: &mut Context<Self>) {
        let folder = self.active.as_deref().and_then(|path| {
            self.folders.iter().filter(|root| calmmd_core::core::paths::is_within(path, root)).max_by_key(|root| root.len())
        }).cloned().or_else(|| self.root.clone());
        if let Some(folder) = folder {
            if let Err(error) = self.close_root(&folder, cx) { self.set_message(error, cx); }
        }
    }

    pub fn close_root(&mut self, folder: &str, cx: &mut Context<Self>) -> Result<(), String> {
        use calmmd_core::core::paths::{is_within, path_key};
        let mut roots = self.folders.clone();
        roots.retain(|root| path_key(root) != path_key(folder));
        if roots.len() == self.folders.len() { return Ok(()); }
        let detached = |path: &str| is_within(path, folder) && !roots.iter().any(|root| is_within(path, root));
        self.services.autosave.detach_paths(&self.window_label, detached)?;
        self.services.workspace.set_roots(&self.window_label, roots.clone())?;
        self.tabs.retain(|path| !detached(path));
        self.closed_tabs.retain(|path| !detached(path));
        let remove_active = self.active.as_deref().is_some_and(detached);
        if self.root.as_deref() == Some(folder) { self.claim_space(None, cx); }
        self.folders = roots;
        self.root = self.folders.first().cloned();
        self.refresh_documents();
        self.refresh_recovery_drafts();
        self.run_search();
        if remove_active {
            self.active = None;
            self.text.clear();
            self.dirty = false;
            self.conflict = false;
            self.autosave_status = "clean".into();
            if let Some(path) = self.tabs.last().cloned() { self.open_document(&path, cx); }
        }
        self.set_message("Folder closed".into(), cx);
        Ok(())
    }

    pub fn recent_folders(&self) -> Vec<String> {
        calmmd_core::kv::read(self.context(), "gpui/recent-folders")
            .ok().flatten().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
    }

    pub fn clear_recent_folders(&self) -> Result<(), String> {
        calmmd_core::kv::write(self.context(), "gpui/recent-folders", "[]")
    }

    pub fn space_name(&self) -> Option<String> {
        let id = self.services.claims.space_for_window(&self.window_label)?;
        self.services.session.space(&id).ok().flatten().map(|space| space.name)
    }

    pub fn save_space(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        use calmmd_core::core::session::{SessionDto, SessionTabDto, SpaceDto};
        if name.trim().is_empty() { return Err("Enter a Space name.".into()); }
        let root = self.root.clone().ok_or("Open a folder before saving a Space.")?;
        let existing = self.services.claims.space_for_window(&self.window_label);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let id = existing.clone().unwrap_or_else(|| {
            let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            format!("gpui-{}-{unique}", std::process::id())
        });
        let created_at = match existing {
            Some(id) => self.services.session.space(&id)?.map_or(now, |space| space.created_at),
            None => now,
        };
        let tabs = self.tabs.iter().map(|path| SessionTabDto { path: Some(path.clone()), draft_id: None, cursor: 0, top_line: 0 }).collect();
        let active = self.tabs.iter().position(|path| Some(path.as_str()) == self.active()).unwrap_or(0);
        self.services.session.save_space(&SpaceDto {
            v: 1, id: id.clone(), name: name.to_owned(), created_at, last_opened: now,
            session: SessionDto { root: Some(root), extra_roots: self.folders.iter().skip(1).cloned().collect(), tabs, active, ..Default::default() },
        })?;
        self.claim_space(Some(&id), cx);
        self.set_message(format!("Space saved: {name}"), cx);
        Ok(())
    }

    pub fn switch_space(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let mut space = self.services.session.space(id)?.ok_or("This Space no longer exists.")?;
        let root = space.session.root.as_deref().filter(|root| StdPath::new(root).is_dir())
            .ok_or("The Space folder is unavailable.")?;
        if self.services.claims.holder(id).is_some_and(|owner| owner != self.window_label) {
            return Err("This Space is already open in another window.".into());
        }
        self.try_open_folder(root.into(), cx)?;
        for extra in &space.session.extra_roots {
            if StdPath::new(extra).is_dir() { self.add_folder(extra.into(), cx)?; }
        }
        self.claim_space(Some(id), cx);
        self.tabs = space.session.tabs.iter().filter_map(|tab| tab.path.clone())
            .filter(|path| StdPath::new(path).is_file() && self.services.workspace.resolve(&self.window_label, path).is_ok())
            .collect();
        if let Some(path) = space.session.tabs.get(space.session.active).and_then(|tab| tab.path.as_ref())
            .filter(|path| self.tabs.contains(path)).cloned().or_else(|| self.tabs.first().cloned()) {
            self.open_document(&path, cx);
        }
        space.last_opened = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        self.services.session.save_space(&space)?;
        self.set_message(format!("Opened Space: {}", space.name), cx);
        Ok(())
    }

    pub fn delete_space(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        if self.services.claims.holder(id).is_some_and(|owner| owner != self.window_label) {
            return Err("Close this Space in its other window before deleting it.".into());
        }
        self.services.session.remove_space(id)?;
        if self.services.claims.space_for_window(&self.window_label).as_deref() == Some(id) {
            self.claim_space(None, cx);
        }
        self.set_message("Space removed".into(), cx);
        Ok(())
    }

    pub fn open_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if let Err(error) = self.add_folder(path, cx) { self.set_message(error, cx); }
    }

    pub fn add_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) -> Result<(), String> {
        use calmmd_core::core::paths::path_key;
        if !path.is_dir() { return Err("The selected folder is unavailable.".into()); }
        let path = path.to_string_lossy().into_owned();
        if !self.folders.iter().any(|root| path_key(root) == path_key(&path)) {
            let mut roots = self.folders.clone();
            roots.push(path.clone());
            self.services.workspace.set_roots(&self.window_label, roots.clone())?;
            self.folders = roots;
            self.root = self.folders.first().cloned();
            self.remember_folder(&path);
            self.refresh_documents();
            self.run_search();
        }
        self.set_message(format!("Folder opened: {path}"), cx);
        Ok(())
    }

    fn remember_folder(&self, path: &str) {
        use calmmd_core::core::paths::path_key;
        let mut recent = self.recent_folders();
        recent.retain(|folder| path_key(folder) != path_key(path));
        recent.insert(0, path.to_owned());
        recent.truncate(12);
        if let Ok(raw) = serde_json::to_string(&recent) {
            let _ = calmmd_core::kv::write(self.context(), "gpui/recent-folders", &raw);
        }
    }

    pub fn try_open_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) -> Result<(), String> {
        if !path.is_dir() {
            return Err("The selected folder is unavailable.".into());
        }
        self.services.autosave.flush_for_folder_switch(&self.window_label)?;
        let path = path.to_string_lossy().into_owned();
        self.services.workspace.sync(
            &self.window_label,
            calmmd_core::core::dto::WorkspaceSnapshotDto {
                root: Some(path.clone()),
                documents: Vec::new(),
                active_document: None,
            },
        )?;
        self.services.autosave.release_saved_owner(&self.window_label);
        self.refresh_recovery_drafts();
        self.claim_space(None, cx);
        self.remember_folder(&path);
        self.folders = vec![path.clone()];
        self.root = Some(path);
        self.closed_tabs.clear();
        self.refresh_documents();
        self.active = None;
        self.tabs.clear();
        self.text = "Select a document, or create a new one.".into();
        self.conflict = false;
        self.dirty = false;
        self.mtime_ms = None;
        self.eol = "\n".into();
        self.bom = false;
        self.autosave_status = "clean".into();
        self.search_results.clear();
        self.message = format!("Folder opened: {}", self.root.as_deref().unwrap_or_default());
        cx.notify();
        Ok(())
    }

    pub fn open_document(&mut self, path: &str, cx: &mut Context<Self>) {
        self.navigate_document(path, false, cx);
    }

    pub fn open_in_new_tab(&mut self, path: &str, cx: &mut Context<Self>) {
        self.navigate_document(path, true, cx);
    }

    fn navigate_document(&mut self, path: &str, new_tab: bool, cx: &mut Context<Self>) {
        match self.services.workspace.read_document(&self.window_label, path) {
            Ok(value) => {
                let Some(path) = value.get("path").and_then(Value::as_str) else {
                    self.set_message("The document has no path.".into(), cx);
                    return;
                };
                let previous = self.active.replace(path.to_owned());
                if let Some(replaced) = place_tab(&mut self.tabs, previous.as_deref(), path, new_tab) {
                    if !self.dirty && !self.conflict {
                        let _ = self.services.workspace.remove_document(&self.window_label, &replaced);
                    }
                }
                self.text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.eol = value.get("eol").and_then(Value::as_str).unwrap_or("\n").to_owned();
                self.bom = value.get("bom").and_then(Value::as_bool).unwrap_or(false);
                self.mtime_ms = value.get("mtimeMs").and_then(Value::as_f64);
                self.conflict = value.get("conflict").and_then(Value::as_bool).unwrap_or(false);
                self.dirty = value.get("dirty").and_then(Value::as_bool).unwrap_or(false);
                if !self.dirty && !self.conflict {
                    if let Some(path) = self.active.as_deref() {
                        self.services.autosave.dispose_if_clean(&autosave_id(&self.window_label, path));
                    }
                }
                self.autosave_status = if self.dirty { "dirty" } else { "clean" }.into();
                self.sync_active();
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
            } else if path.is_file() && calmmd_core::core::paths::is_markdown_path(raw) {
                documents.push(raw.to_owned());
            }
        }
        if let Some(folder) = folders.into_iter().next() {
            self.open_folder(folder, cx);
        }
        if documents.first().is_some_and(|path| !self.folders.iter().any(|root| calmmd_core::core::paths::is_within(path, root))) {
            if let Some(document) = documents.first() {
                if let Some(parent) = StdPath::new(document).parent() {
                    self.open_folder(parent.to_owned(), cx);
                }
            }
        }
        if let Some(document) = documents.first() {
            self.open_document(document, cx);
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

    fn new_document_folder(&self) -> Option<String> {
        self.active.as_deref().and_then(|path| StdPath::new(path).parent())
            .filter(|parent| parent.is_dir())
            .map(|parent| parent.to_string_lossy().into_owned())
            .filter(|parent| self.services.workspace.resolve(&self.window_label, parent).is_ok())
            .or_else(|| self.active_root().map(str::to_owned))
    }

    pub fn create_document(&mut self, name: &str, new_tab: bool, cx: &mut Context<Self>) {
        let result = self.services.workspace.create_document(
            &self.window_label,
            self.new_document_folder().as_deref(),
            name,
            Some("# New note\n"),
        );
        match result {
            Ok(value) => {
                let path = value.get("path").and_then(Value::as_str).unwrap_or(name).to_owned();
                self.refresh_documents();
                self.navigate_document(&path, new_tab, cx);
                self.message = "Document created".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    pub fn create_untitled(&mut self, cx: &mut Context<Self>) {
        self.create_untitled_in_tab(false, cx);
    }

    pub fn create_untitled_in_tab(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(root) = self.new_document_folder() else {
            self.message = "Open a folder before creating a document".into();
            cx.notify();
            return;
        };
        let mut index = 1;
        loop {
            let name = if index == 1 { "Untitled.md".to_owned() } else { format!("Untitled {index}.md") };
            if !StdPath::new(&root).join(&name).exists() {
                self.create_document(&name, new_tab, cx);
                return;
            }
            index += 1;
        }
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
                    id: autosave_id(&self.window_label, &path),
                    owner: Some(self.window_label.clone()),
                    untitled_index: 0,
                    path: Some(path),
                    text,
                    eol: self.eol.clone(),
                    bom: self.bom,
                    mtime_ms: self.mtime_ms,
                    conflict: self.conflict,
                },
                idle_ms: 3000,
                max_wait_ms: 30_000,
                retry_delays_ms: vec![3000, 10_000, 30_000],
            };
            self.services.autosave.touch(request);
            self.autosave_status = "dirty".into();
            self.sync_active();
        }
        cx.notify();
    }

    pub fn save_active(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else {
            self.message = "No document is open".into();
            cx.notify();
            return;
        };
        let state = self.services.autosave.flush(&autosave_id(&self.window_label, &path));
        self.conflict = state.conflict;
        self.autosave_status = state.status.clone();
        if state.conflict {
            self.message = "Save conflict — choose a version".into();
        } else if let Some(error) = state.last_error {
            self.message = error;
        } else {
            self.mtime_ms = state.mtime_ms.or(self.mtime_ms);
            self.dirty = false;
            self.message = "Saved".into();
        }
        self.sync_active();
        cx.notify();
    }

    pub fn restore_version(&mut self, path: &str, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        if self.active() != Some(path) {
            return Err("The active document changed.".into());
        }
        let text = self.services.history.read(path, id)?.ok_or("The saved version is unavailable.")?;
        self.services.history.save(path, &self.text, None)?;
        self.editor_changed(text, cx);
        self.set_message("Version restored".into(), cx);
        Ok(())
    }

    pub fn save_as(&mut self, destination: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(current) = self.active.clone() else {
            self.message = "No document is open".into(); cx.notify(); return;
        };
        let target = destination.to_string_lossy().into_owned();
        if calmmd_core::core::paths::path_key(&target) == calmmd_core::core::paths::path_key(&current) {
            self.save_active(cx);
            return;
        }
        let encoded = calmmd_core::core::workspace::encode_for_disk(&self.text, &self.eol, self.bom);
        if let Err(error) = calmmd_core::fsops::write_text_file(target.clone(), encoded) {
            self.message = format!("Could not save: {error}"); cx.notify(); return;
        }
        self.refresh_documents();
        self.message = format!("Saved a copy to {}", destination.display());
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
        match self.services.autosave.resolve(&autosave_id(&self.window_label, &path), choice) {
            Ok(result) => {
                if let Some(file) = result.file {
                    self.text = file.text;
                    self.eol = file.eol;
                    self.bom = file.bom;
                    self.mtime_ms = Some(file.mtime_ms);
                    self.dirty = false;
                } else if keep_mine {
                    self.mtime_ms = result.state.mtime_ms;
                    self.dirty = result.state.dirty;
                } else {
                    let _ = self.services.workspace.remove_document(&self.window_label, &path);
                    self.active = None;
                    self.text = "Select a document, or create a new one.".into();
        self.conflict = false;
        self.dirty = false;
        self.autosave_status = "clean".into();
                    self.dirty = false;
                }
                self.autosave_status = result.state.status;
                self.conflict = result.state.conflict;
                self.message = if keep_mine { "Kept local changes" } else if self.active.is_none() { "Discarded draft" } else { "Loaded disk version" }.into();
                self.sync_active();
                self.refresh_recovery_drafts();
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
            .and_then(|value| value.as_array().cloned().or_else(|| value.get("hits")?.as_array().cloned()))
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

    pub fn set_spell_language(&mut self, language: Option<String>, cx: &mut Context<Self>) {
        self.spell_language = language;
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
            .and_then(|value| documents_from_value(&value))
            .unwrap_or_default();
    }

    fn apply_workspace_event(&mut self, event: &CoreEvent, cx: &mut Context<Self>) {
        let kind = event.payload.get("kind").and_then(Value::as_str).unwrap_or("");
        let path = event.payload.get("path").and_then(Value::as_str);
        match kind {
            "content" => {
                if let Some(body) = event.payload.get("text").and_then(Value::as_str) {
                    if path == self.active.as_deref() {
                        if self.dirty {
                            self.conflict = true;
                            self.message = "Document changed while editing — choose a version".into();
                            if let Some(mut dto) = self.active_dto() {
                                dto.conflict = true;
                                let _ = self.services.workspace.upsert_document(&self.window_label, dto, self.active.clone());
                            }
                            return;
                        }
                        self.text = body.to_owned();
                        self.dirty = false;
                        self.mtime_ms = event.payload.get("mtimeMs").and_then(Value::as_f64).or(self.mtime_ms);
                        self.sync_active();
                    }
                }
                self.message = "Document changed in Rust".into();
            }
            "renamed" | "moved" => {
                if let (Some(from), Some(to), Some(active)) = (
                    event.payload.get("from").and_then(Value::as_str),
                    event.payload.get("to").and_then(Value::as_str),
                    self.active.as_deref(),
                ) {
                    if calmmd_core::core::paths::is_within(active, from) {
                        self.active = Some(calmmd_core::core::paths::rebase(active, from, to));
                    }
                }
                if let (Some(from), Some(to)) = (event.payload.get("from").and_then(Value::as_str), event.payload.get("to").and_then(Value::as_str)) {
                    for root in &mut self.folders {
                        if calmmd_core::core::paths::is_within(root, from) {
                            *root = calmmd_core::core::paths::rebase(root, from, to);
                        }
                    }
                    self.root = self.folders.first().cloned();
                    let mut favourites = self.favourites();
                    let mut changed = false;
                    for favourite in &mut favourites {
                        if calmmd_core::core::paths::is_within(favourite, from) {
                            *favourite = calmmd_core::core::paths::rebase(favourite, from, to);
                            changed = true;
                        }
                    }
                    if changed { let _ = self.write_favourites(&favourites); }
                    for tab in &mut self.tabs {
                        if calmmd_core::core::paths::is_within(tab, from) { *tab = calmmd_core::core::paths::rebase(tab, from, to); }
                    }
                }
                self.refresh_documents();
                self.message = format!("Workspace changed: {kind}");
            }
            "created" | "trashed" => {
                if kind == "trashed" {
                    if let Some(path) = path {
                        let removed = |candidate: &str| calmmd_core::core::paths::is_within(candidate, path);
                        self.folders.retain(|folder| !removed(folder));
                        self.root = self.folders.first().cloned();
                        self.tabs.retain(|tab| !removed(tab));
                        self.closed_tabs.retain(|tab| !removed(tab));
                        let favourites = self.favourites();
                        let kept: Vec<_> = favourites.iter().filter(|item| !removed(item)).cloned().collect();
                        if favourites.len() != kept.len() { let _ = self.write_favourites(&kept); }
                        if self.active.as_deref().is_some_and(removed) {
                            self.active = None;
                            self.text.clear();
                            self.dirty = false;
                            self.conflict = false;
                            self.autosave_status = "clean".into();
                            if let Some(next) = self.tabs.last().cloned() { self.open_document(&next, cx); }
                        }
                    }
                }
                self.refresh_documents();
                self.message = format!("Workspace changed: {kind}");
            }
            _ => {}
        }
    }

    fn apply_autosave_event(&mut self, event: &CoreEvent) {
        let Some(state) = event.payload.get("state") else { return };
        let id = state.get("id").and_then(Value::as_str);
        let prefix = format!("{}|", self.window_label);
        let path = id.and_then(|id| id.strip_prefix(&prefix));
        if let (Some(id), Some(path)) = (id, path) {
            if let Some(current) = self.services.autosave.status(id) {
                let _ = self.services.workspace.update_autosave(&self.window_label, path, current.dirty, current.conflict, current.mtime_ms);
                if !current.dirty && !current.conflict && self.active() != Some(path)
                    && !self.tabs.iter().any(|tab| tab == path)
                {
                    let _ = self.services.workspace.remove_document(&self.window_label, path);
                    self.services.autosave.dispose_if_clean(id);
                }
            }
        }
        if path != self.active.as_deref() {
            return;
        }
        let Some(current) = id.and_then(|id| self.services.autosave.status(id)) else { return };
        self.autosave_status = current.status;
        self.conflict = current.conflict;
        if let Some(mtime) = current.mtime_ms {
            self.mtime_ms = Some(mtime);
        }
        self.dirty = current.dirty;
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
            eol: self.eol.clone(),
            bom: self.bom,
            mtime_ms: self.mtime_ms,
            dirty: self.dirty,
            conflict: self.conflict,
        })
    }

    fn sync_active(&self) {
        if let Some(doc) = self.active_dto() {
            let _ = self.services.workspace.upsert_document(&self.window_label, doc, self.active.clone());
        }
    }

    pub fn flush_before_close(&mut self, cx: &mut Context<Self>) -> bool {
        match self.services.autosave.flush_owner(&self.window_label) {
            Ok(()) => true,
            Err(error) => { self.message = error; cx.notify(); false }
        }
    }

    pub fn title(&self) -> SharedString {
        match self.active.as_deref() {
            Some(path) => path.rsplit(['/', '\\']).next().unwrap_or(path).into(),
            None => "CalmMD".into(),
        }
    }
}

/// Ordinary navigation uses the active slot. A file already shown in another tab is selected.
fn place_tab(tabs: &mut Vec<String>, active: Option<&str>, path: &str, new_tab: bool) -> Option<String> {
    use calmmd_core::core::paths::path_key;
    let target = path_key(path);
    if tabs.iter().any(|tab| path_key(tab) == target) {
        return None;
    }
    if !new_tab {
        if let Some(slot) = active.and_then(|active| tabs.iter_mut().find(|tab| path_key(tab) == path_key(active))) {
            return Some(std::mem::replace(slot, path.to_owned()));
        }
    }
    tabs.push(path.to_owned());
    None
}

fn parent_dir(path: &str) -> String {
    StdPath::new(path)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn autosave_id(owner: &str, path: &str) -> String {
    format!("{owner}|{path}")
}

fn default_spell_language() -> Option<String> {
    Some("en-US".to_owned())
}

fn documents_from_value(value: &Value) -> Option<Vec<DocumentItem>> {
    let entries = value.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| {
                Some(DocumentItem {
                    path: entry.get("path")?.as_str()?.to_owned(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn tab_navigation_reuses_active_slot_unless_a_new_tab_is_requested() {
        let mut tabs = Vec::new();
        assert_eq!(super::place_tab(&mut tabs, None, "a.md", false), None);
        assert_eq!(super::place_tab(&mut tabs, Some("a.md"), "b.md", false), Some("a.md".into()));
        assert_eq!(tabs, ["b.md"]);
        assert_eq!(super::place_tab(&mut tabs, Some("b.md"), "c.md", true), None);
        assert_eq!(super::place_tab(&mut tabs, Some("c.md"), "b.md", false), None);
        assert_eq!(tabs, ["b.md", "c.md"]);
        assert_eq!(super::place_tab(&mut tabs, Some("b.md"), "d.md", false), Some("b.md".into()));
        assert_eq!(tabs, ["d.md", "c.md"]);
    }

    use super::documents_from_value;
    use serde_json::json;

    #[test]
    fn bundled_english_is_the_default_spell_language() {
        assert_eq!(super::default_spell_language().as_deref(), Some("en-US"));
    }

    #[test]
    fn document_list_is_read_from_the_array_shape_returned_by_core() {
        let value = json!([
            { "path": "a.md", "name": "a.md", "open": false },
            { "path": "b.md", "name": "b.md", "open": true }
        ]);
        let documents = documents_from_value(&value).unwrap();
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[1].path, "b.md");
    }
}
