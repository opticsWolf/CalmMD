//! Rust-owned autosave timing.
//!
//! The webview reports each edit with `core_autosave_touch`. Rust keeps the latest buffer, waits
//! for the idle/max-wait deadline, writes through `BufferStore`, retries failures, and emits status
//! events for the view. The webview no longer owns debounce timers or retry backoff.

use super::buffers::{BufferStore, DecodedFile, PersistRequest};
use super::history::HistoryStore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, RwLock, atomic::{AtomicU64, Ordering}},
    time::{Duration, Instant},
};
use crate::app::AppContext;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveRequest {
    #[serde(flatten)]
    pub buffer: PersistRequest,
    #[serde(default = "default_idle_ms")]
    pub idle_ms: u64,
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
    #[serde(default = "default_retries")]
    pub retry_delays_ms: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveState {
    pub id: String,
    pub status: String,
    pub dirty: bool,
    pub conflict: bool,
    #[serde(default)]
    pub mtime_ms: Option<f64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveEvent {
    pub kind: String,
    pub state: AutosaveState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolveChoice {
    KeepMine,
    LoadTheirs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveResult {
    pub state: AutosaveState,
    #[serde(default)]
    pub file: Option<DecodedFile>,
}

fn default_idle_ms() -> u64 { 3000 }
fn default_max_wait_ms() -> u64 { 30_000 }
fn default_retries() -> Vec<u64> { vec![3000, 10_000, 30_000] }

struct Pending {
    request: PersistRequest,
    idle_ms: u64,
    max_wait_ms: u64,
    retry_delays_ms: Vec<u64>,
    first_dirty_at: Instant,
    generation: u64,
    retry_index: usize,
    state: AutosaveState,
}

impl Pending {
    fn delay(&self) -> Duration {
        let elapsed = self.first_dirty_at.elapsed().as_millis() as u64;
        let budget = self.max_wait_ms.saturating_sub(elapsed);
        Duration::from_millis(self.idle_ms.min(budget))
    }
}

/// Cloneable autosave registry. One instance is managed by Tauri for the whole app.
pub struct AutosaveService<S: HistoryStore + 'static> {
    pending: Arc<RwLock<HashMap<String, Pending>>>,
    serial: Arc<Mutex<()>>,
    generations: Arc<AtomicU64>,
    emit: Arc<dyn Fn(AutosaveEvent) + Send + Sync>,
    store: Arc<BufferStore<S>>,
}

impl<S: HistoryStore + 'static> Clone for AutosaveService<S> {
    fn clone(&self) -> Self {
        Self {
            pending: self.pending.clone(),
            serial: self.serial.clone(),
            generations: self.generations.clone(),
            emit: self.emit.clone(),
            store: self.store.clone(),
        }
    }
}

impl<S: HistoryStore + 'static> AutosaveService<S> {
    pub fn new(store: Arc<BufferStore<S>>, emit: impl Fn(AutosaveEvent) + Send + Sync + 'static) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            serial: Arc::new(Mutex::new(())),
            generations: Arc::new(AtomicU64::new(1)),
            emit: Arc::new(emit),
            store,
        }
    }

    pub fn status(&self, id: &str) -> Option<AutosaveState> {
        self.pending.read().ok()?.get(id).map(|p| p.state.clone())
    }

    /// Records the latest text and (re)schedules the write. Older timers exit on generation change.
    pub fn touch(&self, req: AutosaveRequest) -> AutosaveState {
        let id = req.buffer.id.clone();
        let mut all = match self.pending.write() {
            Ok(all) => all,
            Err(_) => return AutosaveState {
                id,
                status: "error".into(),
                dirty: true,
                conflict: false,
                mtime_ms: None,
                last_error: Some("Autosave lock poisoned".into()),
            },
        };
        let now = Instant::now();
        let pending = all.entry(id.clone()).or_insert_with(|| Pending {
            request: req.buffer.clone(),
            idle_ms: req.idle_ms,
            max_wait_ms: req.max_wait_ms,
            retry_delays_ms: req.retry_delays_ms.clone(),
            first_dirty_at: now,
            generation: 0,
            retry_index: 0,
            state: AutosaveState {
                id: id.clone(),
                status: "clean".into(),
                dirty: false,
                conflict: false,
                mtime_ms: None,
                last_error: None,
            },
        });
        let mut buffer = req.buffer;
        if pending.state.conflict || pending.request.conflict {
            buffer.conflict = true;
        }
        // A save may have finished before the UI receives its notification.
        if !pending.state.dirty && !pending.state.conflict {
            if let Some(mtime) = pending.state.mtime_ms {
                buffer.mtime_ms = Some(mtime);
            }
        }
        pending.request = buffer;
        pending.idle_ms = req.idle_ms;
        pending.max_wait_ms = req.max_wait_ms;
        pending.retry_delays_ms = req.retry_delays_ms;
        if !pending.state.dirty {
            pending.first_dirty_at = now;
        }
        pending.generation = self.generations.fetch_add(1, Ordering::Relaxed);
        pending.retry_index = 0;
        pending.state.dirty = true;
        pending.state.status = "dirty".into();
        pending.state.last_error = None;
        let generation = pending.generation;
        let delay = pending.delay();
        let state = pending.state.clone();
        drop(all);
        self.schedule(id, generation, delay);
        state
    }

    /// Writes immediately, cancelling any pending timer.
    pub fn flush(&self, id: &str) -> AutosaveState {
        let generation = match self.pending.write() {
            Ok(mut all) => match all.get_mut(id) {
                Some(pending) => {
                    pending.generation = self.generations.fetch_add(1, Ordering::Relaxed);
                    pending.generation
                }
                None => return AutosaveState {
                    id: id.into(),
                    status: "clean".into(),
                    dirty: false,
                    conflict: false,
                    mtime_ms: None,
                    last_error: None,
                },
            },
            Err(_) => return AutosaveState {
                id: id.into(),
                status: "error".into(),
                dirty: true,
                conflict: false,
                mtime_ms: None,
                last_error: Some("Autosave lock poisoned".into()),
            },
        };
        self.write(id, generation)
    }

    /// Applies the user's conflict choice using the pending Rust buffer, not a view snapshot.
    pub fn resolve(&self, id: &str, choice: ResolveChoice) -> Result<ResolveResult, String> {
        let path = {
            let all = self.pending.read().map_err(|e| e.to_string())?;
            let pending = all.get(id).ok_or_else(|| "No pending autosave for this document".to_owned())?;
            pending
                .request
                .path
                .clone()
                .ok_or_else(|| "This draft has no file path".to_owned())?
        };
        match choice {
            ResolveChoice::KeepMine => {
                if matches!(std::fs::metadata(&path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound) {
                    let _serial = self.serial.lock().map_err(|e| e.to_string())?;
                    let (request, generation) = {
                        let all = self.pending.read().map_err(|e| e.to_string())?;
                        let pending = all.get(id).ok_or("No pending autosave for this document")?;
                        (pending.request.clone(), pending.generation)
                    };
                    let mtime = self.store.restore_missing(&request)?;
                    let mut all = self.pending.write().map_err(|e| e.to_string())?;
                    let pending = all.get_mut(id).ok_or("No pending autosave for this document")?;
                    pending.request.mtime_ms = Some(mtime);
                    pending.request.conflict = false;
                    pending.state.mtime_ms = Some(mtime);
                    pending.state.conflict = false;
                    if pending.generation == generation {
                        pending.state.dirty = false;
                        pending.state.status = "clean".into();
                    }
                    let state = pending.state.clone();
                    drop(all);
                    (self.emit)(AutosaveEvent { kind: "saved".into(), state: state.clone() });
                    return Ok(ResolveResult { state, file: None });
                }
                let file = self.store.load(&path)?;
                let generation = {
                    let mut all = self.pending.write().map_err(|e| e.to_string())?;
                    let pending = all.get_mut(id).ok_or_else(|| "No pending autosave for this document".to_owned())?;
                    pending.request.conflict = false;
                    pending.request.mtime_ms = Some(file.mtime_ms);
                    pending.generation = self.generations.fetch_add(1, Ordering::Relaxed);
                    pending.generation
                };
                let state = self.write(id, generation);
                if state.status == "error" {
                    return Err(state.last_error.clone().unwrap_or_else(|| "Could not save this document".into()));
                }
                Ok(ResolveResult { state, file: None })
            }
            ResolveChoice::LoadTheirs => {
                let _serial = self.serial.lock().map_err(|e| e.to_string())?;
                let file = match self.store.load(&path) {
                    Ok(file) => Some(file),
                    Err(_) if matches!(std::fs::metadata(&path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound) => None,
                    Err(error) => return Err(error),
                };
                self.store.delete_draft(id)?;
                self.dispose(id);
                let state = AutosaveState {
                    id: id.into(),
                    status: "clean".into(),
                    dirty: false,
                    conflict: false,
                    mtime_ms: file.as_ref().map(|file| file.mtime_ms),
                    last_error: None,
                };
                (self.emit)(AutosaveEvent { kind: "saved".into(), state: state.clone() });
                Ok(ResolveResult { state, file })
            }
        }
    }

    pub fn dispose(&self, id: &str) {
        if let Ok(mut all) = self.pending.write() {
            all.remove(id);
        }
    }

    pub fn dispose_clean_owner(&self, owner: &str) {
        if let Ok(mut all) = self.pending.write() {
            all.retain(|_, pending| pending.request.owner.as_deref() != Some(owner)
                || pending.state.dirty || pending.state.conflict);
        }
    }

    pub fn dispose_if_clean(&self, id: &str) {
        if let Ok(mut all) = self.pending.write() {
            if all.get(id).is_some_and(|pending| !pending.state.dirty && !pending.state.conflict) {
                all.remove(id);
            }
        }
    }

    /// Flush pending edits belonging to a window before closing it.
    pub fn flush_owner(&self, owner: &str) -> Result<(), String> {
        let ids: Vec<String> = self.pending.read().map_err(|e| e.to_string())?
            .iter().filter(|(_, pending)| pending.request.owner.as_deref() == Some(owner)
                && (pending.state.dirty || pending.state.conflict))
            .map(|(id, _)| id.clone()).collect();
        for id in ids {
            let state = self.flush(&id);
            if state.conflict { return Err(format!("Resolve the save conflict in {id} before closing.")); }
            if state.dirty || state.status == "error" {
                return Err(state.last_error.unwrap_or_else(|| format!("Could not save {id}.")));
            }
        }
        Ok(())
    }

    fn schedule(&self, id: String, generation: u64, delay: Duration) {
        let service = self.clone();
        spawn(async move {
            tokio::time::sleep(delay).await;
            service.write_if_current(&id, generation);
        });
    }

    fn write_if_current(&self, id: &str, generation: u64) {
        let current = self.pending.read().ok().and_then(|all| all.get(id).map(|p| p.generation));
        if current == Some(generation) {
            self.write(id, generation);
        }
    }

    fn write(&self, id: &str, generation: u64) -> AutosaveState {
        let _serial = match self.serial.lock() {
            Ok(guard) => guard,
            Err(err) => return AutosaveState { id: id.into(), status: "error".into(), dirty: true,
                conflict: false, mtime_ms: None, last_error: Some(err.to_string()) },
        };
        let (request, retry_delays, retry_index) = {
            let mut all = match self.pending.write() {
                Ok(all) => all,
                Err(_) => return AutosaveState {
                    id: id.into(),
                    status: "error".into(),
                    dirty: true,
                    conflict: false,
                    mtime_ms: None,
                    last_error: Some("Autosave lock poisoned".into()),
                },
            };
            let Some(pending) = all.get_mut(id) else {
                return AutosaveState {
                    id: id.into(),
                    status: "clean".into(),
                    dirty: false,
                    conflict: false,
                    mtime_ms: None,
                    last_error: None,
                };
            };
            if pending.generation != generation {
                return pending.state.clone();
            }
            pending.state.status = "saving".into();
            let state = pending.state.clone();
            (self.emit)(AutosaveEvent { kind: "status".into(), state });
            (pending.request.clone(), pending.retry_delays_ms.clone(), pending.retry_index)
        };

        let written_path = request.path.clone();
        let result = self.store.persist(request);

        let mut all = match self.pending.write() {
            Ok(all) => all,
            Err(_) => return AutosaveState {
                id: id.into(),
                status: "error".into(),
                dirty: true,
                conflict: false,
                mtime_ms: None,
                last_error: Some("Autosave lock poisoned".into()),
            },
        };
        if result.as_ref().is_ok_and(|result| result.wrote) {
            if let Some(path) = written_path.as_deref() {
                for (other_id, other) in all.iter_mut() {
                    if other_id != id && other.state.dirty
                        && other.request.path.as_deref().is_some_and(|other_path|
                            super::paths::path_key(other_path) == super::paths::path_key(path)) {
                        other.request.conflict = true;
                        other.state.conflict = true;
                        (self.emit)(AutosaveEvent { kind: "status".into(), state: other.state.clone() });
                    }
                }
            }
        }
        let Some(pending) = all.get_mut(id) else {
            return AutosaveState {
                id: id.into(),
                status: "clean".into(),
                dirty: false,
                conflict: false,
                mtime_ms: None,
                last_error: None,
            };
        };
        if pending.generation != generation {
            if let Ok(ref saved) = result {
                if saved.wrote {
                    pending.request.mtime_ms = saved.mtime_ms;
                } else if saved.conflict {
                    pending.request.conflict = true;
                }
            }
            return pending.state.clone();
        }
        let result = match result {
            Ok(result) => {
                pending.state.dirty = false;
                pending.state.conflict = result.conflict;
                pending.state.mtime_ms = result.mtime_ms;
                if result.wrote { pending.request.mtime_ms = result.mtime_ms; }
                pending.state.last_error = None;
                pending.state.status = "clean".into();
                pending.retry_index = 0;
                pending.first_dirty_at = Instant::now();
                Ok(pending.state.clone())
            }
            Err(err) => {
                pending.state.dirty = true;
                pending.state.status = "error".into();
                pending.state.last_error = Some(err.clone());
                let delay = retry_delays
                    .get(retry_index)
                    .copied()
                    .unwrap_or_else(|| retry_delays.last().copied().unwrap_or(1000));
                pending.retry_index = retry_index + 1;
                Err((duration_from_ms(delay), err))
            }
        };
        let state = pending.state.clone();
        drop(all);
        match result {
            Ok(_) => (self.emit)(AutosaveEvent { kind: "saved".into(), state: state.clone() }),
            Err((delay, _)) => {
                (self.emit)(AutosaveEvent { kind: "status".into(), state: state.clone() });
                self.schedule(id.into(), generation, delay);
            }
        }
        state
    }
}

/// Reuse an ambient Tokio runtime when present (async tests, MCP server), otherwise use the
/// core's own runtime. Neither UI shell needs to provide one.
fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    } else {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("autosave runtime")
            })
            .spawn(future);
    }
}

fn duration_from_ms(ms: u64) -> Duration {
    Duration::from_millis(ms)
}


pub type AppAutosave = AutosaveService<crate::core::history::AppHistoryStore>;

pub fn create_app_autosave(app: &AppContext) -> AppAutosave {
    let store = Arc::new(BufferStore::new(crate::core::history::AppHistoryStore::new(app.clone())));
    let app = app.clone();
    AutosaveService::new(store, move |event| {
        app.emit("calmmd://autosave", event);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::history::HistoryStore;
    use std::{collections::HashMap, sync::Mutex as StdMutex};

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

    fn request(path: &str) -> AutosaveRequest {
        AutosaveRequest {
            buffer: PersistRequest {
                id: "tab1".into(),
                owner: None,
                untitled_index: 1,
                path: Some(path.into()),
                text: "hello".into(),
                eol: "\n".into(),
                bom: false,
                mtime_ms: None,
                conflict: false,
            },
            idle_ms: 20,
            max_wait_ms: 200,
            retry_delays_ms: vec![20],
        }
    }

    #[tokio::test]
    async fn writes_after_idle_and_emits_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let seen = events.clone();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store, move |event| seen.lock().unwrap().push(event));

        let state = service.touch(request(&path));
        assert_eq!(state.status, "dirty");
        assert!(state.dirty);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        let state = service.status("tab1").unwrap();
        assert_eq!(state.status, "clean");
        assert!(!state.dirty);
        assert!(events.lock().unwrap().iter().any(|event| event.kind == "saved"));
    }

    #[tokio::test]
    async fn resolves_keep_mine_and_load_theirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.md").to_string_lossy().into_owned();
        std::fs::write(&path, "theirs").unwrap();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store, |_| {});

        let mut mine = request(&path);
        mine.buffer.text = "mine".into();
        mine.buffer.mtime_ms = Some(1.0);
        mine.buffer.conflict = true;
        mine.idle_ms = 10_000;
        service.touch(mine.clone());
        let kept = service.resolve("tab1", ResolveChoice::KeepMine).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
        assert!(!kept.state.conflict);
        assert!(kept.file.is_none());

        service.touch(mine);
        let loaded = service.resolve("tab1", ResolveChoice::LoadTheirs).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
        assert_eq!(loaded.file.unwrap().text, "mine");
        assert!(service.status("tab1").is_none());
    }

    #[tokio::test]
    async fn load_theirs_removes_the_conflict_draft() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conflict.md").to_string_lossy().into_owned();
        std::fs::write(&path, "theirs").unwrap();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut edit = request(&path);
        edit.buffer.conflict = true;
        service.touch(edit);
        assert!(service.flush("tab1").conflict);
        assert_eq!(store.drafts().unwrap().len(), 1);
        service.resolve("tab1", ResolveChoice::LoadTheirs).unwrap();
        assert!(store.drafts().unwrap().is_empty());
    }

    #[tokio::test]
    async fn closing_flushes_dirty_documents_and_blocks_on_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("close.md").to_string_lossy().into_owned();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut edit = request(&path);
        edit.buffer.owner = Some("main".into());
        edit.idle_ms = 10_000;
        service.touch(edit.clone());
        service.flush_owner("main").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        edit.buffer.text = "unsaved conflict".into();
        edit.buffer.conflict = true;
        service.touch(edit);
        assert!(service.flush_owner("main").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        assert_eq!(store.drafts().unwrap()[0].record.text, "unsaved conflict");
    }

    #[tokio::test]
    async fn concurrent_window_edits_remain_separate_and_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.md").to_string_lossy().into_owned();
        std::fs::write(&path, "original").unwrap();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut first = request(&path);
        first.buffer.id = "first|shared".into();
        first.buffer.owner = Some("first".into());
        first.buffer.text = "first text".into();
        first.idle_ms = 10_000;
        let mut second = first.clone();
        second.buffer.id = "second|shared".into();
        second.buffer.owner = Some("second".into());
        second.buffer.text = "second text".into();
        service.touch(first);
        service.touch(second);
        service.flush_owner("first").unwrap();
        assert!(service.flush_owner("second").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first text");
        assert_eq!(store.drafts().unwrap()[0].record.text, "second text");
    }

    #[tokio::test]
    async fn old_timer_cannot_save_a_reopened_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reopen.md").to_string_lossy().into_owned();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut edit = request(&path);
        edit.idle_ms = 10_000;
        service.touch(edit.clone());
        let old_generation = service.pending.read().unwrap()["tab1"].generation;
        service.flush("tab1");
        service.dispose_if_clean("tab1");
        edit.buffer.text = "new edit".into();
        edit.buffer.mtime_ms = Some(store.load(&path).unwrap().mtime_ms);
        service.touch(edit);
        service.write_if_current("tab1", old_generation);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        assert!(service.status("tab1").unwrap().dirty);
    }

    #[tokio::test]
    async fn missing_file_conflict_can_be_restored_or_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("removed.md").to_string_lossy().into_owned();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut edit = request(&path);
        edit.buffer.mtime_ms = Some(1.0);
        edit.buffer.eol = "\r\n".into();
        edit.buffer.text = "a\nb\n".into();
        edit.idle_ms = 10_000;
        service.touch(edit.clone());
        assert!(service.flush("tab1").conflict);
        let restored = service.resolve("tab1", ResolveChoice::KeepMine).unwrap();
        assert!(!restored.state.conflict);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb\r\n");
        assert!(store.drafts().unwrap().is_empty());
        std::fs::remove_file(&path).unwrap();
        edit.buffer.conflict = true;
        service.touch(edit);
        service.flush("tab1");
        let discarded = service.resolve("tab1", ResolveChoice::LoadTheirs).unwrap();
        assert!(discarded.file.is_none());
        assert!(store.drafts().unwrap().is_empty());
        assert!(!std::path::Path::new(&path).exists());
    }

    #[tokio::test]
    async fn retouching_replaces_the_pending_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.md").to_string_lossy().into_owned();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store, |_| {});

        service.touch(request(&path));
        let mut second = request(&path);
        second.buffer.text = "newer".into();
        service.touch(second);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "newer");
    }

    #[tokio::test]
    async fn continuous_edits_do_not_reset_max_wait() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("continuous.md").to_string_lossy().into_owned();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let service = AutosaveService::new(store, |_| {});
        let mut edit = request(&path);
        edit.idle_ms = 100;
        edit.max_wait_ms = 120;
        service.touch(edit.clone());
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(30)).await;
            service.touch(edit.clone());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello");
    }

    #[tokio::test]
    async fn detects_external_changes_after_a_save_before_ui_receives_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("external.md").to_string_lossy().into_owned();
        std::fs::write(&path, "original").unwrap();
        let store = Arc::new(BufferStore::new(MemoryStore::default()));
        let initial = store.load(&path).unwrap();
        let service = AutosaveService::new(store.clone(), |_| {});
        let mut edit = request(&path);
        edit.buffer.mtime_ms = Some(initial.mtime_ms);
        edit.idle_ms = 10_000;
        service.touch(edit.clone());
        assert!(!service.flush("tab1").conflict);
        std::fs::write(&path, "external").unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now() + Duration::from_secs(5))).unwrap();
        edit.buffer.text = "new local text".into();
        service.touch(edit);
        assert!(service.flush("tab1").conflict);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
        assert_eq!(store.drafts().unwrap()[0].record.text, "new local text");
    }
}
