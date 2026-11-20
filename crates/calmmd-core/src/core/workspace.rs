//! Rust-owned workspace state.
//!
//! The webview mirrors its live buffers here; MCP tools call these services directly instead of
//! round-tripping through `calmmd://mcp/request`. Mutations emit events so the view can catch up.

use super::{
    dto::{WorkspaceDocumentDto, WorkspaceEventDto, WorkspaceSnapshotDto},
    history::{AppHistoryStore, History},
    links::rewrite_links,
    markdown,
    paths::{basename, dirname, is_markdown_path, is_within, join_path, path_key, rebase, validate_name, with_markdown_ext},
};
use crate::{fsops, tree};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}},
    time::UNIX_EPOCH,
};
use crate::app::AppContext;

const MAX_FILES: usize = 5000;
const DEFAULT_WINDOW: &str = "main";

#[derive(Debug, Clone, Default)]
struct Workspace {
    root: Option<String>,
    docs: HashMap<String, WorkspaceDocumentDto>,
    active: Option<String>,
}

impl Workspace {
    fn document(&self, path: &str) -> Option<&WorkspaceDocumentDto> {
        self.docs.get(&path_key(path))
    }

    fn document_mut(&mut self, path: &str) -> Option<&mut WorkspaceDocumentDto> {
        self.docs.get_mut(&path_key(path))
    }

    fn retarget(&mut self, from: &str, to: &str) {
        let docs = std::mem::take(&mut self.docs);
        for (_, mut doc) in docs {
            if is_within(&doc.path, from) {
                doc.path = rebase(&doc.path, from, to);
            }
            self.docs.insert(path_key(&doc.path), doc);
        }
        if let Some(active) = &self.active {
            if is_within(active, from) {
                self.active = Some(rebase(active, from, to));
            }
        }
    }

    fn close_within(&mut self, path: &str) {
        self.docs.retain(|_, doc| !is_within(&doc.path, path));
        if self.active.as_ref().is_some_and(|active| is_within(active, path)) {
            self.active = None;
        }
    }
}

/// Pure workspace operations. Generic over the history store so file/history rules are testable.
#[derive(Debug, Clone)]
struct TextChange {
    before: String,
    after: String,
}

pub struct WorkspaceStore<S: crate::core::history::HistoryStore> {
    window: RwLock<HashMap<String, Workspace>>,
    history: History<S>,
    undo: RwLock<HashMap<String, Vec<TextChange>>>,
    redo: RwLock<HashMap<String, Vec<TextChange>>>,
    reject_dirty_tool_writes: AtomicBool,
}

impl<S: crate::core::history::HistoryStore> WorkspaceStore<S> {
    pub fn with_history(history: History<S>) -> Arc<Self> {
        Arc::new(Self {
            window: RwLock::new(HashMap::new()),
            history,
            undo: RwLock::new(HashMap::new()),
            redo: RwLock::new(HashMap::new()),
            reject_dirty_tool_writes: AtomicBool::new(false),
        })
    }

    fn read_lock(&self) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, Workspace>>, String> {
        self.window.read().map_err(|e| e.to_string())
    }

    fn write_lock(&self) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, Workspace>>, String> {
        self.window.write().map_err(|e| e.to_string())
    }

    fn workspace(&self, _window: &str) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, Workspace>>, String> {
        self.read_lock()
    }

    pub fn sync(&self, window: &str, snapshot: WorkspaceSnapshotDto) -> Result<(), String> {
        let mut all = self.write_lock()?;
        let state = all.entry(window.to_owned()).or_default();
        state.root = snapshot.root;
        state.docs = snapshot
            .documents
            .into_iter()
            .map(|doc| (path_key(&doc.path), doc))
            .collect();
        state.active = snapshot.active_document;
        Ok(())
    }

    pub fn upsert_document(&self, window: &str, document: WorkspaceDocumentDto, active: Option<String>) -> Result<(), String> {
        let mut all = self.write_lock()?;
        let state = all.entry(window.to_owned()).or_default();
        state.docs.insert(path_key(&document.path), document);
        if let Some(active) = active {
            state.active = Some(active);
        }
        Ok(())
    }

    pub fn update_autosave(&self, window: &str, path: &str, dirty: bool, conflict: bool, mtime: Option<f64>) -> Result<(), String> {
        let mut all = self.write_lock()?;
        if let Some(doc) = all.get_mut(window).and_then(|state| state.document_mut(path)) {
            doc.dirty = dirty;
            doc.conflict = conflict;
            if mtime.is_some() { doc.mtime_ms = mtime; }
        }
        Ok(())
    }

    pub fn remove_document(&self, window: &str, path: &str) -> Result<(), String> {
        let mut all = self.write_lock()?;
        if let Some(state) = all.get_mut(window) {
            state.docs.remove(&path_key(path));
            if state.active.as_deref().is_some_and(|active| path_key(active) == path_key(path)) { state.active = None; }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn history(&self) -> &History<S> {
        &self.history
    }

    fn root_of(&self, window: &str) -> Result<String, String> {
        let all = self.workspace(window)?;
        all.get(window)
            .and_then(|state| state.root.clone())
            .filter(|root| !root.is_empty())
            .ok_or_else(|| "Open a folder in CalmMD first".into())
    }

    pub(crate) fn resolve(&self, window: &str, raw: &str) -> Result<String, String> {
        let root = self.root_of(window)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("Missing path".into());
        }
        let absolute = trimmed.starts_with('/')
            || trimmed.starts_with('\\')
            || (trimmed.len() >= 3 && trimmed.as_bytes()[1] == b':' && (trimmed.as_bytes()[2] == b'/' || trimmed.as_bytes()[2] == b'\\'));
        let path = if absolute {
            trimmed.to_owned()
        } else {
            join_path(&root, trimmed)
        };
        let normalised = path.replace('\\', "/");
        if normalised.split('/').any(|segment| segment == "..") {
            return Err("Paths may not contain \"..\"".into());
        }
        if !is_within(&path, &root) {
            return Err("That path is outside the open folder".into());
        }
        // Canonicalize the nearest existing ancestor too, so a new file beneath a link
        // cannot escape the workspace. Broken links are rejected rather than skipped.
        let canonical_root = fs::canonicalize(&root).map_err(|e| fsops::describe(&e))?;
        let mut existing = Path::new(&path);
        loop {
            match fs::symlink_metadata(existing) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or("Invalid workspace path")?;
                }
                Err(error) => return Err(fsops::describe(&error)),
            }
        }
        let canonical = fs::canonicalize(existing).map_err(|e| fsops::describe(&e))?;
        if !canonical.starts_with(&canonical_root) {
            return Err("That path is outside the open folder".into());
        }
        Ok(path)
    }

    fn live_or_disk(&self, window: &str, path: &str) -> Result<(String, String, bool, f64), String> {
        let all = self.workspace(window)?;
        if let Some(doc) = all.get(window).and_then(|state| state.document(path)).filter(|doc| doc.dirty || doc.conflict) {
            return Ok((
                doc.text.clone(),
                doc.eol.clone(),
                doc.bom,
                doc.mtime_ms.unwrap_or_else(|| mtime_ms(path)),
            ));
        }
        drop(all);
        let raw = fs::read_to_string(path).map_err(|e| fsops::describe(&e))?;
        let decoded = decode_from_disk(&raw);
        Ok((decoded.text, decoded.eol, decoded.bom, mtime_ms(path)))
    }

    pub fn status(&self, window: &str, label: &str, windows: Value) -> Result<Value, String> {
        let all = self.workspace(window)?;
        let state = all.get(window);
        let (root, open_documents, active) = match state {
            Some(state) => (
                state.root.clone(),
                state
                    .docs
                    .values()
                    .filter(|doc| !doc.path.is_empty())
                    .map(|doc| doc.path.clone())
                    .collect::<Vec<_>>(),
                state.active.clone(),
            ),
            None => (None, Vec::new(), None),
        };
        let window_labels: Vec<String> = windows
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.get("label").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Ok(json!({
            "windowLabel": label,
            "windows": if window_labels.is_empty() { windows } else { json!(window_labels) },
            "root": root,
            "openDocuments": open_documents,
            "activeDocument": active,
        }))
    }

    fn collect_files(&self, root: &str, folder: Option<&str>, markdown_only: bool) -> Result<Vec<String>, String> {
        let start = folder.unwrap_or(root);
        if !is_within(start, root) {
            return Err("That folder is outside the open folder".into());
        }
        let mut out = Vec::new();
        let mut queue = vec![start.to_owned()];
        let mut visited = std::collections::HashSet::new();
        while let Some(dir) = queue.pop() {
            let Ok(canonical) = fs::canonicalize(&dir) else { continue };
            if !visited.insert(canonical) { continue; }
            let entries = match tree::list(Path::new(&dir), false) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries {
                if self.resolve_path_for_listing(root, &entry.path).is_err() { continue; }
                if entry.is_dir {
                    if out.len() < MAX_FILES {
                        queue.push(entry.path);
                    }
                } else if !markdown_only || is_markdown_path(&entry.name) {
                    out.push(entry.path);
                    if out.len() >= MAX_FILES {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    fn resolve_path_for_listing(&self, root: &str, path: &str) -> Result<(), String> {
        let root = fs::canonicalize(root).map_err(|e| fsops::describe(&e))?;
        let path = fs::canonicalize(path).map_err(|e| fsops::describe(&e))?;
        if !path.starts_with(root) { return Err("That path is outside the open folder".into()); }
        Ok(())
    }

    pub fn list_documents(&self, window: &str, markdown_only: bool, folder: Option<&str>) -> Result<Value, String> {
        let root = self.root_of(window)?;
        let folder = match folder {
            Some(folder) => Some(self.resolve(window, folder)?),
            None => None,
        };
        let files = self.collect_files(&root, folder.as_deref(), markdown_only)?;
        let all = self.read_lock()?;
        let open = all
            .get(window)
            .map(|state| state.docs.keys().cloned().collect::<std::collections::HashSet<_>>())
            .unwrap_or_default();
        Ok(Value::Array(
            files
                .into_iter()
                .map(|path| {
                    json!({
                        "path": path,
                        "name": basename(&path),
                        "open": open.contains(&path_key(&path)),
                    })
                })
                .collect(),
        ))
    }

    pub fn read_document(&self, window: &str, raw_path: &str) -> Result<Value, String> {
        let path = self.resolve(window, raw_path)?;
        let (text, eol, bom, mtime) = self.live_or_disk(window, &path)?;
        let all = self.read_lock()?;
        let doc = all.get(window).and_then(|state| state.document(&path));
        Ok(json!({
            "path": path,
            "text": text,
            "eol": eol,
            "bom": bom,
            "mtimeMs": mtime,
            "dirty": doc.is_some_and(|doc| doc.dirty),
            "conflict": doc.is_some_and(|doc| doc.conflict),
            "headings": outline(&text),
        }))
    }

    pub fn outline(&self, window: &str, raw_path: &str) -> Result<Value, String> {
        let path = self.resolve(window, raw_path)?;
        let (text, _, _, _) = self.live_or_disk(window, &path)?;
        Ok(json!({ "path": path, "headings": outline(&text) }))
    }

    pub fn search(&self, window: &str, query: &str, case_sensitive: bool, whole_word: bool) -> Result<Value, String> {
        let root = self.root_of(window)?;
        serde_json::to_value(crate::search::search(Path::new(&root), query, case_sensitive, whole_word))
            .map_err(|e| e.to_string())
    }

    fn ensure_folder(&self, window: &str, folder: &str) -> Result<(), String> {
        let root = self.root_of(window)?;
        if !is_within(folder, &root) {
            return Err("That path is outside the open folder".into());
        }
        fs::create_dir_all(folder).map_err(|e| fsops::describe(&e))
    }

    pub fn create_document(&self, window: &str, folder: Option<&str>, name: &str, text: Option<&str>) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let root = self.root_of(window)?;
        let parent = match folder {
            Some(folder) => self.resolve(window, folder)?,
            None => root.clone(),
        };
        if !is_within(&parent, &root) {
            return Err("That path is outside the open folder".into());
        }
        let name = with_markdown_ext(name);
        validate_name(&name)?;
        let path = join_path(&parent, &name);
        self.resolve(window, &path)?;
        tree::create_file(path.clone())?;
        let mut events = vec![WorkspaceEventDto::created(path.clone(), false)];
        let mut result = json!({ "path": path });
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            let (value, more) = self.write_document(window, &path, text, "replace", true)?;
            result = value;
            events.extend(more);
        }
        Ok((result, events))
    }

    pub fn create_folder(&self, window: &str, folder: Option<&str>, name: &str) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let root = self.root_of(window)?;
        let parent = match folder {
            Some(folder) => self.resolve(window, folder)?,
            None => root.clone(),
        };
        if !is_within(&parent, &root) {
            return Err("That path is outside the open folder".into());
        }
        let name = validate_name(name)?;
        let path = join_path(&parent, &name);
        fs::create_dir(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "A folder with that name already exists.".into()
            } else {
                fsops::describe(&e)
            }
        })?;
        Ok((json!({ "path": path }), vec![WorkspaceEventDto::created(path, true)]))
    }

    pub fn write_document(
        &self,
        window: &str,
        raw_path: &str,
        content: &str,
        mode: &str,
        flush: bool,
    ) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let _ = flush; // Rust writes synchronously; the flag is kept for tool compatibility.
        self.apply_text(window, raw_path, content, mode, true)
    }

    /// Applies one text change. `journal` is false for undo/redo so replaying a change doesn't
    /// push a new undo entry while that step is already moving between stacks.
    fn apply_text(
        &self,
        window: &str,
        raw_path: &str,
        content: &str,
        mode: &str,
        journal: bool,
    ) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let path = self.resolve(window, raw_path)?;
        let (current, eol, bom, _old_mtime) = self.live_or_disk(window, &path)?;
        {
            let all = self.workspace(window)?;
            if let Some(doc) = all.get(window).and_then(|state| state.document(&path)) {
                if doc.dirty && self.reject_dirty_tool_writes.load(Ordering::Relaxed) {
                    return Err("This document has unsaved edits. Save it in CalmMD before changing it through MCP.".into());
                }
                if doc.conflict {
                    return Err("This document has a conflict. Resolve it in CalmMD before editing it.".into());
                }
                if let Some(mtime) = doc.mtime_ms {
                    if mtime_ms(&path) != mtime {
                        return Err("The file changed on disk; reload it in CalmMD before editing it.".into());
                    }
                }
            }
        }
        let next = match mode {
            "append" => format!("{current}{content}"),
            "prepend" => format!("{content}{current}"),
            _ => content.to_owned(),
        };
        if next == current {
            return Ok((json!({ "path": path, "changed": false }), Vec::new()));
        }
        if current.trim().len() > 0 {
            self.history.save(&path, &current, None)?;
        }
        self.history.save(&path, &next, None)?;
        let encoded = encode_for_disk(&next, &eol, bom);
        let new_mtime = write_text(&path, &encoded)?;
        {
            let mut all = self.write_lock()?;
            if let Some(state) = all.get_mut(window) {
                let doc = state.docs.entry(path_key(&path)).or_insert_with(|| WorkspaceDocumentDto {
                    path: path.clone(),
                    text: next.clone(),
                    eol: eol.clone(),
                    bom,
                    mtime_ms: Some(new_mtime),
                    dirty: false,
                    conflict: false,
                });
                doc.text = next.clone();
                doc.mtime_ms = Some(new_mtime);
                doc.dirty = false;
                doc.conflict = false;
                doc.eol = eol;
                doc.bom = bom;
            }
        }
        if journal {
            let mut undo = self.undo.write().map_err(|e| e.to_string())?;
            undo.entry(path_key(&path)).or_default().push(TextChange {
                before: current,
                after: next.clone(),
            });
            let mut redo = self.redo.write().map_err(|e| e.to_string())?;
            redo.remove(&path_key(&path));
        }
        Ok((
            json!({ "path": path, "changed": true }),
            vec![WorkspaceEventDto::content(path, next, new_mtime)],
        ))
    }

    fn jump_history(&self, window: &str, raw_path: &str, redo: bool) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let path = self.resolve(window, raw_path)?;
        let key = path_key(&path);
        let change = {
            let stacks = if redo { self.redo.write() } else { self.undo.write() };
            let mut stacks = stacks.map_err(|e| e.to_string())?;
            let stack = stacks.get_mut(&key).ok_or_else(|| if redo { "Nothing to redo" } else { "Nothing to undo" })?;
            stack.pop().ok_or_else(|| if redo { "Nothing to redo" } else { "Nothing to undo" })?
        };
        let target = if redo { change.after.clone() } else { change.before.clone() };
        let (_value, events) = match self.apply_text(window, &path, &target, "replace", false) {
            Ok(result) => result,
            Err(error) => {
                let stacks = if redo { &self.redo } else { &self.undo };
                stacks.write().map_err(|e| e.to_string())?.entry(key).or_default().push(change);
                return Err(error);
            }
        };
        let stacks = if redo { self.undo.write() } else { self.redo.write() };
        let mut stacks = stacks.map_err(|e| e.to_string())?;
        // Both stacks keep the original direction: undo jumps to `before`, redo to `after`.
        stacks.entry(key).or_default().push(change);
        Ok((json!({ "path": path, "changed": true, "text": target, "redo": redo }), events))
    }

    pub fn undo_document(&self, window: &str, raw_path: &str) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        self.jump_history(window, raw_path, false)
    }

    pub fn redo_document(&self, window: &str, raw_path: &str) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        self.jump_history(window, raw_path, true)
    }

    fn retarget_journal(&self, from: &str, to: &str) {
        for journal in [&self.undo, &self.redo] {
            let Ok(mut all) = journal.write() else { continue };
            let old = std::mem::take(&mut *all);
            for (path, changes) in old {
                let key = if is_within(&path, from) { rebase(&path, from, to) } else { path };
                all.entry(key).or_default().extend(changes);
            }
        }
    }

    fn move_path(&self, window: &str, from: &str, to: &str) -> Result<Vec<WorkspaceEventDto>, String> {
        // Rewrites may affect any document, so do not move while a GPUI buffer has
        // an outstanding save. This also prevents a pending save recreating `from`.
        if self.reject_dirty_tool_writes.load(Ordering::Relaxed)
            && self.read_lock()?.values().any(|state| state.docs.values().any(|doc| doc.dirty || doc.conflict)) {
            return Err("Save or resolve open documents before moving or renaming files.".into());
        }
        tree::rename(Path::new(from), Path::new(to)).map_err(|e| e.to_string())?;
        self.history.rename(from, to)?;
        self.retarget_journal(from, to);
        let mut all = self.write_lock()?;
        for state in all.values_mut() {
            state.retarget(from, to);
        }
        drop(all);

        let root = self.root_of(window)?;
        let files = self.collect_files(&root, None, true)?;
        let mut events = vec![WorkspaceEventDto::renamed(from.to_owned(), to.to_owned())];
        for path in &files {
            let path = path.clone();
            let (before, eol, bom, _) = match self.live_or_disk(window, &path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let after = rewrite_links(&before, &path, from, to, &files);
            if after == before {
                continue;
            }
            if before.trim().len() > 0 {
                self.history.save(&path, &before, None)?;
            }
            self.history.save(&path, &after, None)?;
            let mtime = write_text(&path, &encode_for_disk(&after, &eol, bom))?;
            let mut all = self.write_lock()?;
            if let Some(state) = all.get_mut(window) {
                if let Some(doc) = state.document_mut(&path) {
                    doc.text = after.clone();
                    doc.mtime_ms = Some(mtime);
                    doc.dirty = false;
                    doc.conflict = false;
                }
            }
            drop(all);
            events.push(WorkspaceEventDto::content(path, after, mtime));
        }
        Ok(events)
    }

    pub fn rename_document(&self, window: &str, raw_path: &str, name: &str) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let path = self.resolve(window, raw_path)?;
        let name = validate_name(name)?;
        if name.contains('/') || name.contains('\\') {
            return Err("A file name may not contain a path separator".into());
        }
        let to = join_path(&dirname(&path), &name);
        if path_key(&path) == path_key(&to) {
            return Ok((json!({ "path": to }), Vec::new()));
        }
        if Path::new(&to).exists() {
            return Err("Something with that name already exists.".into());
        }
        let events = self.move_path(window, &path, &to)?;
        Ok((json!({ "path": to }), events))
    }

    pub fn move_document(&self, window: &str, raw_path: &str, raw_folder: &str, name: Option<&str>) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let path = self.resolve(window, raw_path)?;
        let folder = self.resolve(window, raw_folder)?;
        self.ensure_folder(window, &folder)?;
        let fallback = basename(&path);
        let name = name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(&fallback);
        let name = validate_name(name)?;
        if name.contains('/') || name.contains('\\') {
            return Err("A file name may not contain a path separator".into());
        }
        let to = join_path(&folder, &name);
        if path_key(&path) == path_key(&to) {
            return Ok((json!({ "from": path, "to": to }), Vec::new()));
        }
        if Path::new(&to).exists() {
            return Err("Something with that name already exists.".into());
        }
        let events = self.move_path(window, &path, &to)?;
        Ok((json!({ "from": path, "to": to }), events))
    }

    pub fn trash_document(&self, window: &str, raw_path: &str, confirm: bool) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        if !confirm {
            return Err("Move to trash requires confirm: true".into());
        }
        let path = self.resolve(window, raw_path)?;
        let is_dir = fs::metadata(&path).map(|meta| meta.is_dir()).map_err(|e| fsops::describe(&e))?;
        trash::delete(&path).map_err(|e| e.to_string())?;
        let mut all = self.write_lock()?;
        if let Some(state) = all.get_mut(window) {
            state.close_within(&path);
        }
        drop(all);
        Ok((
            json!({ "path": path, "trashed": true }),
            vec![WorkspaceEventDto::trashed(path, is_dir)],
        ))
    }

    pub fn triage_folder(&self, window: &str, folder: Option<&str>, limit: usize, excerpt_chars: usize) -> Result<Value, String> {
        let root = self.root_of(window)?;
        let folder = match folder {
            Some(folder) => self.resolve(window, folder)?,
            None => root.clone(),
        };
        let entries: Vec<_> = tree::list(Path::new(&folder), false).map_err(|e| fsops::describe(&e))?
            .into_iter().filter(|entry| self.resolve_path_for_listing(&root, &entry.path).is_ok()).collect();
        let files: Vec<_> = entries.iter().filter(|entry| !entry.is_dir && is_markdown_path(&entry.name)).take(limit).collect();
        let all = self.read_lock()?;
        let open = all
            .get(window)
            .map(|state| state.docs.keys().cloned().collect::<std::collections::HashSet<_>>())
            .unwrap_or_default();
        drop(all);
        let mut notes = Vec::new();
        for entry in &files {
            let path = &entry.path;
            let (text, _, _, _) = self.live_or_disk(window, path)?;
            let headings = outline(&text);
            let parsed = markdown::parse_document(&text).ok();
            let front_matter = parsed.as_ref().map(|p| p.front_matter.clone()).unwrap_or_else(|| json!({}));
            let title = headings
                .first()
                .and_then(|h| h.get("text").and_then(Value::as_str))
                .map(str::to_owned)
                .or_else(|| front_matter.get("title").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| basename(path).trim_end_matches(".md").to_owned());
            let chars: Vec<char> = text.chars().collect();
            let excerpt: String = chars.iter().take(excerpt_chars).collect();
            notes.push(json!({
                "path": path,
                "name": entry.name,
                "title": title,
                "headings": headings.iter().take(8).filter_map(|h| h.get("text").and_then(Value::as_str)).collect::<Vec<_>>(),
                "frontMatter": front_matter,
                "excerpt": excerpt,
                "mtimeMs": mtime_ms(path),
                "open": open.contains(&path_key(path)),
            }));
        }
        let total = entries.iter().filter(|entry| !entry.is_dir && is_markdown_path(&entry.name)).count();
        Ok(json!({
            "folder": folder,
            "subfolders": entries.iter().filter(|entry| entry.is_dir).map(|entry| entry.name.clone()).collect::<Vec<_>>(),
            "notes": notes,
            "total": total,
            "truncated": total > files.len(),
            "limit": limit,
            "excerptChars": excerpt_chars,
        }))
    }

    pub fn apply_organization(
        &self,
        window: &str,
        folder: Option<&str>,
        plan: &Value,
        apply: bool,
    ) -> Result<(Value, Vec<WorkspaceEventDto>), String> {
        let root = self.root_of(window)?;
        let folder = match folder {
            Some(folder) => self.resolve(window, folder)?,
            None => root.clone(),
        };
        let moves = plan.get("moves").and_then(Value::as_array).ok_or("Missing organization plan")?;
        if moves.is_empty() {
            return Err("Organization plan has no moves".into());
        }
        let mut prepared = Vec::new();
        for value in moves.iter() {
            let from = self.resolve(window, value.get("from").and_then(Value::as_str).ok_or("Missing move source")?)?;
            if !is_within(&from, &folder) {
                return Err("A move source is outside the folder being organised".into());
            }
            let to_folder = self.resolve(window, value.get("toFolder").and_then(Value::as_str).ok_or("Missing move destination")?)?;
            if !is_within(&to_folder, &root) {
                return Err("A move destination is outside the open folder".into());
            }
            let fallback = basename(&from);
            let name = value
                .get("newName")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(str::trim)
                .unwrap_or(&fallback);
            let name = validate_name(name)?;
            let to = join_path(&to_folder, &name);
            prepared.push((from, to_folder, name.to_owned(), to, value.get("reason").and_then(Value::as_str).unwrap_or("").to_owned()));
        }
        let mut targets = std::collections::HashSet::new();
        for (_, _, _, to, _) in &prepared {
            if !targets.insert(path_key(to)) {
                return Err(format!("Duplicate destination: {to}"));
            }
        }
        let preview: Vec<Value> = prepared
            .iter()
            .map(|(from, to_folder, name, to, reason)| json!({ "from": from, "toFolder": to_folder, "newName": name, "to": to, "reason": reason }))
            .collect();
        let warning = "Folders are created and notes are moved through CalmMD; links are rewritten by the app. Check the status bar for failures.";
        if !apply {
            return Ok((json!({ "folder": folder, "applied": false, "plan": preview, "warning": warning }), Vec::new()));
        }
        let existing: std::collections::HashSet<String> = self
            .collect_files(&root, None, false)?
            .into_iter()
            .map(|path| path_key(&path))
            .collect();
        let sources: std::collections::HashSet<String> = prepared.iter().map(|(from, _, _, _, _)| path_key(from)).collect();
        for (_, _, _, to, _) in &prepared {
            if existing.contains(&path_key(to)) && !sources.contains(&path_key(to)) {
                return Err(format!("Destination already exists: {to}"));
            }
        }
        let mut events = Vec::new();
        let mut applied = Vec::new();
        let mut failed = Vec::new();
        for (from, to_folder, name, to, reason) in prepared {
            if let Err(error) = self.ensure_folder(window, &to_folder) {
                failed.push(json!({ "from": from, "to": to, "error": error }));
                continue;
            }
            match self.move_path(window, &from, &to) {
                Ok(mut more) => {
                    events.append(&mut more);
                    applied.push(json!({ "from": from, "toFolder": to_folder, "newName": name, "to": to, "reason": reason }));
                }
                Err(error) => failed.push(json!({ "from": from, "to": to, "error": error })),
            }
        }
        Ok((
            json!({ "folder": folder, "applied": true, "plan": applied, "failed": failed, "warning": warning }),
            events,
        ))
    }
}

/// Tauri-managed facade: calls the pure store and emits resulting events to the target window.
#[derive(Clone)]
pub struct WorkspaceRegistry {
    store: Arc<WorkspaceStore<AppHistoryStore>>,
    app: AppContext,
}

impl WorkspaceRegistry {
    pub fn new(app: AppContext) -> Arc<Self> {
        Arc::new(Self {
            store: WorkspaceStore::with_history(History::new(app.clone())),
            app,
        })
    }

    pub fn reject_dirty_tool_writes(&self) {
        self.store.reject_dirty_tool_writes.store(true, Ordering::Relaxed);
    }

    pub fn window_label(target: Option<&str>) -> String {
        target.filter(|label| !label.is_empty()).unwrap_or(DEFAULT_WINDOW).to_owned()
    }

    pub fn sync(&self, window: &str, snapshot: WorkspaceSnapshotDto) -> Result<(), String> {
        self.store.sync(window, snapshot)
    }

    pub fn upsert_document(&self, window: &str, document: WorkspaceDocumentDto, active: Option<String>) -> Result<(), String> {
        self.store.upsert_document(window, document, active)
    }

    pub fn remove_window(&self, window: &str) -> Result<(), String> {
        self.store.write_lock()?.remove(window);
        Ok(())
    }

    pub fn update_autosave(&self, window: &str, path: &str, dirty: bool, conflict: bool, mtime: Option<f64>) -> Result<(), String> {
        self.store.update_autosave(window, path, dirty, conflict, mtime)
    }

    pub fn remove_document(&self, window: &str, path: &str) -> Result<(), String> {
        self.store.remove_document(window, path)
    }

    fn emit(&self, window: &str, events: Vec<WorkspaceEventDto>) {
        for event in events {
            if event.kind == "renamed" || event.kind == "moved" {
                self.app.emit("calmmd://workspace", event);
            } else {
                self.app.emit_to(window, "calmmd://workspace", event);
            }
        }
    }

    fn act<T>(&self, window: &str, result: Result<(T, Vec<WorkspaceEventDto>), String>) -> Result<T, String> {
        let (value, events) = result?;
        self.emit(window, events);
        Ok(value)
    }

    pub fn status(&self, window: &str, label: &str, windows: Value) -> Result<Value, String> {
        self.store.status(window, label, windows)
    }

    pub fn list_documents(&self, window: &str, markdown_only: bool, folder: Option<&str>) -> Result<Value, String> {
        self.store.list_documents(window, markdown_only, folder)
    }

    pub fn read_document(&self, window: &str, path: &str) -> Result<Value, String> {
        self.store.read_document(window, path)
    }

    pub fn resolve(&self, window: &str, path: &str) -> Result<String, String> {
        self.store.resolve(window, path)
    }

    pub fn undo_document(&self, window: &str, path: &str) -> Result<Value, String> {
        let result = self.store.undo_document(window, path)?;
        self.emit(window, result.1);
        Ok(result.0)
    }

    pub fn redo_document(&self, window: &str, path: &str) -> Result<Value, String> {
        let result = self.store.redo_document(window, path)?;
        self.emit(window, result.1);
        Ok(result.0)
    }

    pub fn outline(&self, window: &str, path: &str) -> Result<Value, String> {
        self.store.outline(window, path)
    }

    pub fn search(&self, window: &str, query: &str, case_sensitive: bool, whole_word: bool) -> Result<Value, String> {
        self.store.search(window, query, case_sensitive, whole_word)
    }

    pub fn triage_folder(&self, window: &str, folder: Option<&str>, limit: usize, excerpt_chars: usize) -> Result<Value, String> {
        self.store.triage_folder(window, folder, limit, excerpt_chars)
    }

    pub fn create_document(&self, window: &str, folder: Option<&str>, name: &str, text: Option<&str>) -> Result<Value, String> {
        let result = self.store.create_document(window, folder, name, text);
        self.act(window, result)
    }

    pub fn create_folder(&self, window: &str, folder: Option<&str>, name: &str) -> Result<Value, String> {
        let result = self.store.create_folder(window, folder, name);
        self.act(window, result)
    }

    pub fn write_document(&self, window: &str, path: &str, content: &str, mode: &str, flush: bool) -> Result<Value, String> {
        let result = self.store.write_document(window, path, content, mode, flush);
        self.act(window, result)
    }

    pub fn rename_document(&self, window: &str, path: &str, name: &str) -> Result<Value, String> {
        let result = self.store.rename_document(window, path, name);
        self.act(window, result)
    }

    pub fn move_document(&self, window: &str, path: &str, folder: &str, name: Option<&str>) -> Result<Value, String> {
        let result = self.store.move_document(window, path, folder, name);
        self.act(window, result)
    }

    pub fn trash_document(&self, window: &str, path: &str, confirm: bool) -> Result<Value, String> {
        let result = self.store.trash_document(window, path, confirm);
        self.act(window, result)
    }

    pub fn apply_organization(&self, window: &str, folder: Option<&str>, plan: &Value, apply: bool) -> Result<Value, String> {
        let result = self.store.apply_organization(window, folder, plan, apply);
        self.act(window, result)
    }
}

fn mtime_ms(path: &str) -> f64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn write_text(path: &str, text: &str) -> Result<f64, String> {
    fsops::write_atomic(Path::new(path), text.as_bytes()).map_err(|e| fsops::describe(&e))?;
    Ok(mtime_ms(path))
}

/// Normalises file content the way `decodeFromDisk` does in TypeScript.
pub fn decode_from_disk(raw: &str) -> crate::core::dto::DecodedText {
    let bom = raw.starts_with('\u{feff}');
    let body = if bom { &raw[3..] } else { raw };
    let crlf = body.matches("\r\n").count();
    let lf = body.matches('\n').count() - crlf;
    crate::core::dto::DecodedText {
        text: body.replace("\r\n", "\n").replace('\r', "\n"),
        eol: if crlf > lf { "\r\n" } else { "\n" }.to_owned(),
        bom,
    }
}

pub fn encode_for_disk(text: &str, eol: &str, bom: bool) -> String {
    let body = if eol == "\r\n" { text.replace('\n', "\r\n") } else { text.to_owned() };
    if bom { format!("\u{feff}{body}") } else { body }
}

fn outline(text: &str) -> Vec<Value> {
    match markdown::parse_document(text) {
        Ok(parsed) => parsed
            .headings
            .into_iter()
            .map(|heading| json!({ "level": heading.level, "text": heading.text, "line": heading.line }))
            .collect(),
        Err(_) => Vec::new(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::history::{HistoryStore, HISTORY_KEEP_UNNAMED};
    use std::sync::Mutex as StdMutex;

    #[derive(Clone, Default)]
    struct MemoryStore {
        values: Arc<StdMutex<HashMap<String, String>>>,
    }

    impl HistoryStore for MemoryStore {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.values.lock().unwrap().insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(key);
            Ok(())
        }
        fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
            let mut keys: Vec<String> = self
                .values
                .lock()
                .unwrap()
                .keys()
                .filter(|key| key.starts_with(prefix))
                .cloned()
                .collect();
            keys.sort();
            Ok(keys)
        }
    }

    fn store() -> Arc<WorkspaceStore<MemoryStore>> {
        WorkspaceStore::with_history(History::with_store(MemoryStore::default()))
    }

    #[test]
    fn undo_and_redo_restore_tool_edits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let path = join_path(&root, "note.md");
        fs::write(&path, "one").unwrap();
        let store = store();
        store
            .sync(
                "main",
                WorkspaceSnapshotDto {
                    root: Some(root),
                    documents: vec![WorkspaceDocumentDto {
                        path: path.clone(),
                        text: "one".into(),
                        eol: "\n".into(),
                        bom: false,
                        mtime_ms: None,
                        dirty: false,
                        conflict: false,
                    }],
                    active_document: Some(path.clone()),
                },
            )
            .unwrap();

        store.write_document("main", &path, "two", "replace", true).unwrap();
        let (undone, _) = store.undo_document("main", &path).unwrap();
        assert_eq!(undone["text"], "one");
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        let (redone, _) = store.redo_document("main", &path).unwrap();
        assert_eq!(redone["text"], "two");
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
    }

    #[test]
    fn status_without_a_synced_window_is_empty() {
        let store = store();
        let status = store.status("main", "main", json!([])).unwrap();
        assert_eq!(status["root"], Value::Null);
        assert_eq!(status["windows"], json!([]));
        assert_eq!(status["openDocuments"], json!([]));
    }

    #[test]
    fn resolve_rejects_paths_outside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let store = store();
        store
            .sync(
                "main",
                WorkspaceSnapshotDto {
                    root: Some(root.clone()),
                    documents: Vec::new(),
                    active_document: None,
                },
            )
            .unwrap();
        assert_eq!(store.resolve("main", "sub/a.md").unwrap(), join_path(&root, "sub/a.md"));
        assert!(store.resolve("main", "../secret.md").is_err());
        assert!(store.resolve("main", dir.path().parent().unwrap().to_str().unwrap()).is_err());
    }

    #[test]
    fn linked_paths_cannot_escape_the_workspace() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "secret").unwrap();
        let link = root.path().join("link");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(outside.path(), &link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(outside.path(), &link);
        if linked.is_err() { return; } // Windows may not permit creating symlinks.
        let store = store();
        store.sync("main", WorkspaceSnapshotDto {
            root: Some(root.path().to_string_lossy().into_owned()),
            documents: Vec::new(), active_document: None,
        }).unwrap();
        assert!(store.read_document("main", "link/secret.md").is_err());
        assert!(store.resolve("main", "link/new.md").is_err());
        assert!(store.list_documents("main", true, None).unwrap().as_array().unwrap().is_empty());
    }

    #[test]
    fn triage_excludes_symlinks_to_external_notes() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.md");
        fs::write(&secret, "external secret").unwrap();
        fs::write(root.path().join("safe.md"), "safe").unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&secret, root.path().join("linked.md"));
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&secret, root.path().join("linked.md"));
        if linked.is_err() { return; }
        let store = store();
        store.sync("main", WorkspaceSnapshotDto { root: Some(root.path().to_string_lossy().into_owned()), documents: vec![], active_document: None }).unwrap();
        let triage = store.triage_folder("main", None, 100, 100).unwrap();
        assert_eq!(triage["total"], 1);
        assert!(!triage.to_string().contains("external secret"));
    }

    #[test]
    fn read_document_returns_disk_format_and_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "\u{feff}a\r\nb\r\n").unwrap();
        let store = store();
        store.sync("main", WorkspaceSnapshotDto {
            root: Some(dir.path().to_string_lossy().into_owned()),
            documents: Vec::new(), active_document: None,
        }).unwrap();
        let doc = store.read_document("main", "a.md").unwrap();
        assert_eq!(doc["text"], "a\nb\n");
        assert_eq!(doc["eol"], "\r\n");
        assert_eq!(doc["bom"], true);
        assert!(doc["mtimeMs"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn creation_rejects_existing_document_without_touching_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "original").unwrap();
        let store = store();
        store.sync("main", WorkspaceSnapshotDto { root: Some(dir.path().to_string_lossy().into_owned()), documents: vec![], active_document: None }).unwrap();
        assert!(store.create_document("main", None, "a.md", Some("replacement")).is_err());
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "original");
    }

    #[test]
    fn dirty_mirror_is_readable_and_protected_from_tool_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();
        fs::write(&path, "disk").unwrap();
        let store = store();
        store.reject_dirty_tool_writes.store(true, Ordering::Relaxed);
        store.sync("main", WorkspaceSnapshotDto { root: Some(dir.path().to_string_lossy().into_owned()),
            documents: vec![WorkspaceDocumentDto { path: path.clone(), text: "unsaved".into(), eol: "\n".into(), bom: false, mtime_ms: None, dirty: true, conflict: false }], active_document: Some(path.clone()) }).unwrap();
        let read = store.read_document("main", &path).unwrap();
        assert_eq!(read["text"], "unsaved");
        assert_eq!(read["dirty"], true);
        assert!(store.write_document("main", &path, "tool", "replace", true).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "disk");
    }

    #[test]
    fn clean_mirror_reads_external_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();
        fs::write(&path, "external").unwrap();
        let store = store();
        store.sync("main", WorkspaceSnapshotDto { root: Some(dir.path().to_string_lossy().into_owned()),
            documents: vec![WorkspaceDocumentDto { path: path.clone(), text: "old".into(), eol: "\n".into(), bom: false, mtime_ms: None, dirty: false, conflict: false }], active_document: Some(path.clone()) }).unwrap();
        assert_eq!(store.read_document("main", &path).unwrap()["text"], "external");
    }

    #[test]
    fn rename_is_blocked_until_pending_edits_are_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.md").to_string_lossy().into_owned();
        fs::write(&path, "disk").unwrap();
        let store = store();
        store.reject_dirty_tool_writes.store(true, Ordering::Relaxed);
        store.sync("main", WorkspaceSnapshotDto { root: Some(dir.path().to_string_lossy().into_owned()),
            documents: vec![WorkspaceDocumentDto { path: path.clone(), text: "edited".into(), eol: "\n".into(), bom: false, mtime_ms: None, dirty: true, conflict: false }], active_document: Some(path.clone()) }).unwrap();
        assert!(store.rename_document("main", &path, "new.md").is_err());
        assert!(Path::new(&path).exists());
        store.update_autosave("main", &path, false, false, None).unwrap();
        store.rename_document("main", &path, "new.md").unwrap();
        assert!(!Path::new(&path).exists());
        assert_eq!(store.status("main", "main", json!([])).unwrap()["activeDocument"], dir.path().join("new.md").to_string_lossy().as_ref());
    }

    #[test]
    fn failed_undo_keeps_its_history_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();
        fs::write(&path, "one").unwrap();
        let store = store();
        store.reject_dirty_tool_writes.store(true, Ordering::Relaxed);
        store.sync("main", WorkspaceSnapshotDto { root: Some(dir.path().to_string_lossy().into_owned()), documents: vec![], active_document: None }).unwrap();
        store.write_document("main", &path, "two", "replace", true).unwrap();
        store.update_autosave("main", &path, true, false, None).unwrap();
        assert!(store.undo_document("main", &path).is_err());
        store.update_autosave("main", &path, false, false, None).unwrap();
        store.undo_document("main", &path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
    }

    #[test]
    fn writes_live_text_and_emits_content_events() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let path = join_path(&root, "a.md");
        fs::write(&path, "# Title\n\nold").unwrap();
        let store = store();
        store
            .sync(
                "main",
                WorkspaceSnapshotDto {
                    root: Some(root.clone()),
                    documents: vec![WorkspaceDocumentDto {
                        path: path.clone(),
                        text: "# Title\n\nlive unsaved".into(),
                        eol: "\n".into(),
                        bom: false,
                        mtime_ms: None,
                        dirty: true,
                        conflict: false,
                    }],
                    active_document: Some(path.clone()),
                },
            )
            .unwrap();
        let (result, events) = store
            .write_document("main", "a.md", "\n\nadded", "append", true)
            .unwrap();
        assert_eq!(result["changed"], true);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "content");
        assert!(events[0].text.as_deref().unwrap().ends_with("added"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Title\n\nlive unsaved\n\nadded");
        let versions = store.history().list(&path).unwrap();
        assert_eq!(versions.len(), 2); // before and after
        assert!(versions.len() <= HISTORY_KEEP_UNNAMED);
        let read = store.read_document("main", "a.md").unwrap();
        assert_eq!(read["headings"][0]["text"], "Title");
    }

    #[test]
    fn rename_retargets_live_documents_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let from = join_path(&root, "old.md");
        fs::write(&from, "old").unwrap();
        let store = store();
        store
            .sync(
                "main",
                WorkspaceSnapshotDto {
                    root: Some(root),
                    documents: vec![WorkspaceDocumentDto {
                        path: from.clone(),
                        text: "old".into(),
                        eol: "\n".into(),
                        bom: false,
                        mtime_ms: None,
                        dirty: false,
                        conflict: false,
                    }],
                    active_document: Some(from.clone()),
                },
            )
            .unwrap();
        store.write_document("main", "old.md", "new", "replace", true).unwrap();
        let (result, events) = store.rename_document("main", "old.md", "new.md").unwrap();
        assert_eq!(result["path"], join_path(&dirname(&from), "new.md"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "renamed");
        let status = store.status("main", "main", json!([])).unwrap();
        assert_eq!(status["activeDocument"], result["path"]);
        assert_eq!(store.history().list(&result["path"].as_str().unwrap()).unwrap().len(), 2);
    }

    #[test]
    fn rename_rewrites_wiki_links_after_the_file_moved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let a = join_path(&root, "a.md");
        fs::write(&a, "See [[b]] and [b](b.md).\n").unwrap();
        fs::write(join_path(&root, "b.md"), "# B\n").unwrap();
        let store = store();
        store
            .sync(
                "main",
                WorkspaceSnapshotDto {
                    root: Some(root),
                    documents: Vec::new(),
                    active_document: None,
                },
            )
            .unwrap();
        store.rename_document("main", "b.md", "plan.md").unwrap();
        let text = fs::read_to_string(&a).unwrap();
        assert_eq!(text.trim_end(), "See [[plan]] and [b](plan.md).");
    }

    #[test]
    fn encodes_and_decodes_eol_and_bom() {
        let decoded = decode_from_disk("\u{feff}a\r\nb\r\nc\nd");
        assert!(decoded.bom);
        assert_eq!(decoded.eol, "\r\n");
        assert_eq!(decoded.text, "a\nb\nc\nd");
        assert_eq!(encode_for_disk(&decoded.text, &decoded.eol, decoded.bom), "\u{feff}a\r\nb\r\nc\r\nd");
    }

    #[test]
    fn outline_uses_zero_based_lines() {
        let headings = outline("# Title\n\n## Section\n");
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0]["level"], 1);
        assert_eq!(headings[0]["line"], 0);
        assert_eq!(headings[1]["line"], 2);
    }
}
