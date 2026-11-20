//! Rust-owned file history. The KV layout is identical to `src/app/history.ts`, so versions
//! written by the old webview controller remain visible and vice versa during the migration.

use super::paths::{is_within, path_key, rebase};
use crate::kv;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use crate::app::AppContext;
use uuid::Uuid;

const MAP: &str = "history/map";
/// Unnamed versions kept per file; named versions are never pruned.
pub const HISTORY_KEEP_UNNAMED: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub saved_at: i64,
    pub name: Option<String>,
    /// UTF-16 code units, like `String.length` in TypeScript.
    pub size: usize,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileMapEntry {
    id: String,
    path: String,
}

type FileMap = HashMap<String, FileMapEntry>;

/// Minimal key/value interface so history rules can be tested without a Tauri app.
pub trait HistoryStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, String>;
}

#[derive(Clone)]
pub struct AppHistoryStore {
    app: AppContext,
}

impl AppHistoryStore {
    pub fn new(app: AppContext) -> Self {
        Self { app }
    }
}

impl HistoryStore for AppHistoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        kv::read(&self.app, key)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        kv::write(&self.app, key, value)
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        let dir = self.app.state_dir()?;
        let file = dir.join(kv::key_to_file(key)?);
        match std::fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(crate::fsops::describe(&e)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let dir = self.app.state_dir()?;
        let mut keys: Vec<String> = std::fs::read_dir(dir)
            .map_err(|e| crate::fsops::describe(&e))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| kv::file_to_key(&entry.file_name().to_string_lossy()))
            .filter(|key| key.starts_with(prefix))
            .collect();
        keys.sort();
        Ok(keys)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn hash_text(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn index_key(file_id: &str) -> String {
    format!("history/{file_id}/index")
}

fn text_key(file_id: &str, version_id: &str) -> String {
    format!("history/{file_id}/{version_id}")
}

pub struct History<S: HistoryStore> {
    store: S,
    /// One change at a time, so two saves never race on the same index.
    serial: Arc<Mutex<()>>,
    now: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl<S: HistoryStore> Clone for History<S>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            serial: self.serial.clone(),
            now: self.now.clone(),
        }
    }
}

impl History<AppHistoryStore> {
    pub fn new(app: AppContext) -> Self {
        Self::with_store(AppHistoryStore::new(app))
    }
}

impl<S: HistoryStore> History<S> {
    pub fn with_store(store: S) -> Self {
        Self {
            store,
            serial: Arc::new(Mutex::new(())),
            now: Arc::new(now_ms),
        }
    }

    #[cfg(test)]
    pub fn with_now(store: S, now: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            store,
            serial: Arc::new(Mutex::new(())),
            now: Arc::new(now),
        }
    }

    fn read_json<T: serde::de::DeserializeOwned>(&self, key: &str, fallback: T) -> T {
        match self.store.get(key) {
            Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or(fallback),
            _ => fallback,
        }
    }

    fn read_map(&self) -> FileMap {
        self.read_json(MAP, HashMap::new())
    }

    fn read_index(&self, file_id: &str) -> Vec<Version> {
        self.read_json(&index_key(file_id), Vec::new())
    }

    fn file_id_of(&self, path: &str, create: bool) -> Result<Option<String>, String> {
        let mut map = self.read_map();
        if let Some(entry) = map.get(&path_key(path)) {
            return Ok(Some(entry.id.clone()));
        }
        if !create {
            return Ok(None);
        }
        let id = Uuid::new_v4().simple().to_string();
        map.insert(
            path_key(path),
            FileMapEntry {
                id: id.clone(),
                path: path.to_owned(),
            },
        );
        self.store.set(MAP, &serde_json::to_string(&map).map_err(|e| e.to_string())?)?;
        Ok(Some(id))
    }

    /// Versions of a file, newest first.
    pub fn list(&self, path: &str) -> Result<Vec<Version>, String> {
        let Some(file_id) = self.file_id_of(path, false)? else {
            return Ok(Vec::new());
        };
        let mut versions = self.read_index(&file_id);
        versions.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        Ok(versions)
    }

    pub fn read(&self, path: &str, version_id: &str) -> Result<Option<String>, String> {
        let Some(file_id) = self.file_id_of(path, false)? else {
            return Ok(None);
        };
        self.store.get(&text_key(&file_id, version_id))
    }

    /// Saves a version. Without a name, nothing is stored when the text equals the latest version.
    pub fn save(&self, path: &str, text: &str, name: Option<&str>) -> Result<Option<Version>, String> {
        let _guard = self.serial.lock().map_err(|e| e.to_string())?;
        let hash = hash_text(text);
        let file_id = self.file_id_of(path, true)?.ok_or("Could not create history entry")?;
        let index = self.read_index(&file_id);
        let latest = index.iter().max_by_key(|version| version.saved_at);
        let label = name.map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned);
        if label.is_none() && latest.is_some_and(|version| version.hash == hash) {
            return Ok(None);
        }
        let mut saved_at = (self.now)();
        if let Some(latest) = latest {
            if saved_at <= latest.saved_at {
                saved_at = latest.saved_at + 1;
            }
        }
        let version = Version {
            id: format!("{saved_at:036}-{}", &Uuid::new_v4().simple().to_string()[..4]),
            saved_at,
            name: label,
            size: text.encode_utf16().count(),
            hash,
        };
        self.store
            .set(&text_key(&file_id, &version.id), text)
            .map_err(|e| e.to_string())?;
        let mut next = index;
        next.push(version.clone());
        let mut unnamed: Vec<&Version> = next.iter().filter(|version| version.name.is_none()).collect();
        unnamed.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        let dropped: Vec<String> = unnamed
            .into_iter()
            .skip(HISTORY_KEEP_UNNAMED)
            .map(|version| version.id.clone())
            .collect();
        for id in &dropped {
            self.store.delete(&text_key(&file_id, id))?;
        }
        next.retain(|version| !dropped.contains(&version.id));
        self.store
            .set(&index_key(&file_id), &serde_json::to_string(&next).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Some(version))
    }

    pub fn remove(&self, path: &str, version_id: &str) -> Result<(), String> {
        let _guard = self.serial.lock().map_err(|e| e.to_string())?;
        let Some(file_id) = self.file_id_of(path, false)? else {
            return Ok(());
        };
        self.store.delete(&text_key(&file_id, version_id))?;
        let next: Vec<Version> = self
            .read_index(&file_id)
            .into_iter()
            .filter(|version| version.id != version_id)
            .collect();
        self.store
            .set(&index_key(&file_id), &serde_json::to_string(&next).map_err(|e| e.to_string())?)
    }

    /// Retargets history when a file or folder was renamed/moved.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        let _guard = self.serial.lock().map_err(|e| e.to_string())?;
        let mut map = self.read_map();
        let mut changed = false;
        let keys: Vec<String> = map.keys().cloned().collect();
        for key in keys {
            let Some(entry) = map.get(&key).cloned() else { continue };
            if !is_within(&entry.path, from) {
                continue;
            }
            let path = rebase(&entry.path, from, to);
            map.remove(&key);
            map.insert(path_key(&path), FileMapEntry { id: entry.id, path });
            changed = true;
        }
        if changed {
            self.store
                .set(MAP, &serde_json::to_string(&map).map_err(|e| e.to_string())?)?;
        }
        Ok(())
    }

    pub fn clear_file(&self, path: &str) -> Result<(), String> {
        let _guard = self.serial.lock().map_err(|e| e.to_string())?;
        let mut map = self.read_map();
        let key = path_key(path);
        let Some(entry) = map.remove(&key) else {
            return Ok(());
        };
        for version in self.read_index(&entry.id) {
            self.store.delete(&text_key(&entry.id, &version.id))?;
        }
        self.store.delete(&index_key(&entry.id))?;
        self.store.set(MAP, &serde_json::to_string(&map).map_err(|e| e.to_string())?)
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let _guard = self.serial.lock().map_err(|e| e.to_string())?;
        for key in self.store.list("history/")? {
            self.store.delete(&key)?;
        }
        Ok(())
    }

    pub fn usage(&self) -> Result<HistoryUsage, String> {
        let map = self.read_map();
        let mut usage = HistoryUsage::default();
        for entry in map.values() {
            let versions = self.read_index(&entry.id);
            if versions.is_empty() {
                continue;
            }
            usage.files += 1;
            usage.versions += versions.len();
            usage.chars += versions.iter().map(|version| version.size).sum::<usize>();
        }
        Ok(usage)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryUsage {
    pub files: usize,
    pub versions: usize,
    pub chars: usize,
}

pub type AppHistory = History<AppHistoryStore>;

pub fn app_history(app: &AppContext) -> AppHistory {
    History::new(app.clone())
}


#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn saves_versions_and_skips_identical_text() {
        let history = History::with_now(MemoryStore::default(), || 1000);
        assert_eq!(history.list("C:/notes/a.md").unwrap().len(), 0);
        let first = history.save("C:/notes/a.md", "hello", None).unwrap().unwrap();
        assert_eq!(first.size, 5);
        assert_eq!(first.saved_at, 1000);
        assert!(history.save("C:/notes/a.md", "hello", None).unwrap().is_none());
        let second = history.save("C:/notes/a.md", "changed", None).unwrap().unwrap();
        assert_eq!(second.saved_at, 1001);
        let named = history.save("C:/notes/a.md", "changed", Some(" review ")).unwrap().unwrap();
        assert_eq!(named.name.as_deref(), Some("review"));
        assert_eq!(history.list("C:\\notes\\a.md").unwrap().len(), 3);
        assert_eq!(history.read("C:/notes/a.md", &first.id).unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn rename_moves_history_with_the_file() {
        let history = History::with_now(MemoryStore::default(), || 1000);
        history.save("C:/notes/Book/one.md", "one", None).unwrap();
        history.save("C:/notes/two.md", "two", None).unwrap();
        history.rename("C:/notes/Book", "C:/notes/Archive").unwrap();
        assert_eq!(history.list("C:/notes/Archive/one.md").unwrap().len(), 1);
        assert_eq!(history.list("C:/notes/Book/one.md").unwrap().len(), 0);
        assert_eq!(history.list("C:/notes/two.md").unwrap().len(), 1);
    }

    #[test]
    fn removes_and_clears_versions() {
        let history = History::with_now(MemoryStore::default(), || 1000);
        let version = history.save("a.md", "text", None).unwrap().unwrap();
        history.remove("a.md", &version.id).unwrap();
        assert!(history.list("a.md").unwrap().is_empty());
        history.save("a.md", "again", None).unwrap();
        history.clear_file("a.md").unwrap();
        assert!(history.list("a.md").unwrap().is_empty());
        history.save("b.md", "b", None).unwrap();
        history.clear_all().unwrap();
        assert!(history.list("b.md").unwrap().is_empty());
    }
}
