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
    sync::{Arc, Mutex, RwLock},
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

fn default_idle_ms() -> u64 { 1000 }
fn default_max_wait_ms() -> u64 { 10_000 }
fn default_retries() -> Vec<u64> { vec![1000, 3000, 10_000, 30_000] }

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
    emit: Arc<dyn Fn(AutosaveEvent) + Send + Sync>,
    store: Arc<BufferStore<S>>,
}

impl<S: HistoryStore + 'static> Clone for AutosaveService<S> {
    fn clone(&self) -> Self {
        Self {
            pending: self.pending.clone(),
            serial: self.serial.clone(),
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
        pending.request = req.buffer;
        pending.idle_ms = req.idle_ms;
        pending.max_wait_ms = req.max_wait_ms;
        pending.retry_delays_ms = req.retry_delays_ms;
        pending.first_dirty_at = now.max(pending.first_dirty_at);
        pending.generation = pending.generation.wrapping_add(1);
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
                    pending.generation = pending.generation.wrapping_add(1);
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
        let file = self.store.load(&path)?;
        match choice {
            ResolveChoice::KeepMine => {
                let generation = {
                    let mut all = self.pending.write().map_err(|e| e.to_string())?;
                    let pending = all.get_mut(id).ok_or_else(|| "No pending autosave for this document".to_owned())?;
                    pending.request.conflict = false;
                    pending.request.mtime_ms = Some(file.mtime_ms);
                    pending.generation = pending.generation.wrapping_add(1);
                    pending.generation
                };
                let state = self.write(id, generation);
                if state.status == "error" {
                    return Err(state.last_error.clone().unwrap_or_else(|| "Could not save this document".into()));
                }
                Ok(ResolveResult { state, file: None })
            }
            ResolveChoice::LoadTheirs => {
                self.dispose(id);
                let state = AutosaveState {
                    id: id.into(),
                    status: "clean".into(),
                    dirty: false,
                    conflict: false,
                    mtime_ms: Some(file.mtime_ms),
                    last_error: None,
                };
                (self.emit)(AutosaveEvent { kind: "saved".into(), state: state.clone() });
                Ok(ResolveResult { state, file: Some(file) })
            }
        }
    }

    pub fn dispose(&self, id: &str) {
        if let Ok(mut all) = self.pending.write() {
            all.remove(id);
        }
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

        let result = match self.serial.lock() {
            Ok(_) => self.store.persist(request),
            Err(err) => Err(err.to_string()),
        };

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
        let result = match result {
            Ok(result) => {
                pending.state.dirty = false;
                pending.state.conflict = result.conflict;
                pending.state.mtime_ms = result.mtime_ms;
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
}
