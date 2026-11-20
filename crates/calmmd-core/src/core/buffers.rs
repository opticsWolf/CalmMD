//! Rust-owned draft and autosave persistence rules.
//!
//! The webview still schedules autosave and owns the EditorState, but it no longer decides what
//! "save" means. It hands the current buffer to this service, which owns the KV draft records,
//! detects on-disk conflicts and performs the atomic write. Draft keys and JSON stay compatible
//! with the old webview store, so a running app can be upgraded without losing unsaved work.

use super::history::HistoryStore;
use super::workspace::{decode_from_disk, encode_for_disk};
use crate::{fsops, kv};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, time::SystemTime};
use crate::app::AppContext;

pub const DRAFT_PREFIX: &str = "draft/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRecord {
    pub v: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub text: String,
    pub untitled_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub updated_at: u128,
    #[serde(default = "default_eol")]
    pub eol: String,
    #[serde(default)]
    pub bom: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftEntry {
    pub id: String,
    pub record: DraftRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistRequest {
    pub id: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub untitled_index: u32,
    #[serde(default)]
    pub path: Option<String>,
    pub text: String,
    #[serde(default = "default_eol")]
    pub eol: String,
    #[serde(default)]
    pub bom: bool,
    #[serde(default)]
    pub mtime_ms: Option<f64>,
    #[serde(default)]
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistResult {
    #[serde(default)]
    pub mtime_ms: Option<f64>,
    #[serde(default)]
    pub conflict: bool,
    #[serde(default)]
    pub wrote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<DraftRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedFile {
    pub text: String,
    pub eol: String,
    pub bom: bool,
    pub mtime_ms: f64,
}

fn default_eol() -> String {
    "\n".into()
}

fn draft_key(id: &str) -> Result<String, String> {
    let key = format!("{DRAFT_PREFIX}{id}");
    if kv::key_to_file(&key).is_ok() { return Ok(key); }
    // GPUI uses full paths as autosave IDs; encode them into a safe, bounded KV key.
    use sha2::{Digest, Sha256};
    Ok(format!("{DRAFT_PREFIX}path-{:x}", Sha256::digest(id.as_bytes())))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn mtime_ms(path: &str) -> f64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn safe_path(path: &str) -> Result<(), String> {
    let normalised = path.replace('\\', "/");
    if normalised.split('/').any(|part| part == "..") {
        return Err("Paths may not contain \"..\"".into());
    }
    Ok(())
}

/// Pure buffer rules, generic over the same KV interface used by file history.
pub struct BufferStore<S: HistoryStore> {
    store: S,
}

impl<S: HistoryStore> BufferStore<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn write_draft(&self, id: &str, record: &DraftRecord) -> Result<(), String> {
        let raw = serde_json::to_string(record).map_err(|e| e.to_string())?;
        self.store.set(&draft_key(id)?, &raw)
    }

    pub fn delete_draft(&self, id: &str) -> Result<(), String> {
        self.store.delete(&draft_key(id)?)
    }

    pub fn drafts(&self) -> Result<Vec<DraftEntry>, String> {
        let mut out = Vec::new();
        for key in self.store.list(DRAFT_PREFIX)? {
            let Some(raw) = self.store.get(&key)? else { continue };
            let Ok(record) = serde_json::from_str::<DraftRecord>(&raw) else { continue };
            if record.v != 1 || (record.text.is_empty() && record.path.as_deref().unwrap_or("").is_empty()) {
                continue;
            }
            out.push(DraftEntry {
                id: key.trim_start_matches(DRAFT_PREFIX).to_owned(),
                record,
            });
        }
        out.sort_by(|a, b| a.record.updated_at.cmp(&b.record.updated_at));
        Ok(out)
    }

    /// Applies the autosave decision: write to disk, or keep the text safe in a draft.
    pub fn persist(&self, req: PersistRequest) -> Result<PersistResult, String> {
        let draft = DraftRecord {
            v: 1,
            owner: req.owner.clone(),
            text: req.text.clone(),
            untitled_index: req.untitled_index,
            path: req.path.clone(),
            updated_at: now_ms(),
            eol: req.eol.clone(),
            bom: req.bom,
        };

        let Some(path) = req.path.as_deref().filter(|path| !path.is_empty()) else {
            if req.text.is_empty() {
                self.delete_draft(&req.id)?;
            } else {
                self.write_draft(&req.id, &draft)?;
            }
            return Ok(PersistResult {
                mtime_ms: None,
                conflict: false,
                wrote: false,
                draft: None,
            });
        };
        safe_path(path)?;

        if req.conflict {
            self.write_draft(&req.id, &draft)?;
            return Ok(PersistResult {
                mtime_ms: None,
                conflict: true,
                wrote: false,
                draft: Some(draft),
            });
        }

        let changed_on_disk = match fs::metadata(path) {
            Ok(meta) => {
                let current = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs_f64() * 1000.0);
                req.mtime_ms
                    .zip(current)
                    .is_some_and(|(known, current)| known != current)
            }
            // A previously observed file may have been moved or removed. Preserve the
            // buffer as a conflict instead of recreating its old path.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => req.mtime_ms.is_some(),
            Err(error) => return Err(fsops::describe(&error)),
        };
        if changed_on_disk {
            self.write_draft(&req.id, &draft)?;
            return Ok(PersistResult {
                mtime_ms: None,
                conflict: true,
                wrote: false,
                draft: Some(draft),
            });
        }

        let body = encode_for_disk(&req.text, &req.eol, req.bom);
        fsops::write_atomic(Path::new(path), body.as_bytes()).map_err(|e| fsops::describe(&e))?;
        self.delete_draft(&req.id)?;
        Ok(PersistResult {
            mtime_ms: Some(mtime_ms(path)),
            conflict: false,
            wrote: true,
            draft: None,
        })
    }

    pub fn load(&self, path: &str) -> Result<DecodedFile, String> {
        safe_path(path)?;
        let raw = fs::read_to_string(path).map_err(|e| fsops::describe(&e))?;
        let decoded = decode_from_disk(&raw);
        Ok(DecodedFile {
            text: decoded.text,
            eol: decoded.eol,
            bom: decoded.bom,
            mtime_ms: mtime_ms(path),
        })
    }

    /// Transfer a listed draft to a new buffer ID without destroying another draft.
    pub fn claim_draft(&self, entry: &DraftEntry, id: &str, owner: &str) -> Result<(), String> {
        let old_key = format!("{DRAFT_PREFIX}{}", entry.id);
        let new_key = draft_key(id)?;
        let raw = self.store.get(&old_key)?.ok_or("Draft no longer exists")?;
        if old_key != new_key && self.store.get(&new_key)?.is_some() {
            return Err("Another draft already exists for this document in this window.".into());
        }
        let mut record: DraftRecord = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        record.owner = Some(owner.to_owned());
        self.store.set(&new_key, &serde_json::to_string(&record).map_err(|e| e.to_string())?)?;
        if old_key != new_key { self.store.delete(&old_key)?; }
        Ok(())
    }

    /// Restore a removed file only if nobody has recreated it in the meantime.
    pub fn restore_missing(&self, req: &PersistRequest) -> Result<f64, String> {
        use std::io::Write;
        let path = req.path.as_deref().ok_or("This draft has no file path")?;
        safe_path(path)?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)
            .map_err(|e| fsops::describe(&e))?;
        file.write_all(encode_for_disk(&req.text, &req.eol, req.bom).as_bytes()).map_err(|e| fsops::describe(&e))?;
        file.sync_all().map_err(|e| fsops::describe(&e))?;
        self.delete_draft(&req.id)?;
        Ok(mtime_ms(path))
    }
}


pub fn app_store(app: &AppContext) -> BufferStore<crate::core::history::AppHistoryStore> {
    BufferStore::new(crate::core::history::AppHistoryStore::new(app.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::history::HistoryStore;
    use std::{collections::HashMap, sync::{Arc, Mutex}};

    #[derive(Clone, Default)]
    struct MemoryStore {
        values: Arc<Mutex<HashMap<String, String>>>,
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

    fn request(path: Option<String>, text: &str, mtime: Option<f64>) -> PersistRequest {
        PersistRequest {
            id: "tab1".into(),
            owner: Some("space:main".into()),
            untitled_index: 1,
            path,
            text: text.into(),
            eol: "\n".into(),
            bom: false,
            mtime_ms: mtime,
            conflict: false,
        }
    }

    #[test]
    fn untitled_text_is_kept_as_a_draft_and_removed_when_empty() {
        let store = BufferStore::new(MemoryStore::default());
        store.persist(request(None, "hello", None)).unwrap();
        let drafts = store.drafts().unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].id, "tab1");
        assert_eq!(drafts[0].record.text, "hello");
        assert_eq!(drafts[0].record.owner.as_deref(), Some("space:main"));

        store.persist(request(None, "", None)).unwrap();
        assert!(store.drafts().unwrap().is_empty());
    }

    #[test]
    fn writes_the_file_and_clears_the_draft() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let store = BufferStore::new(MemoryStore::default());
        store.persist(request(None, "draft", None)).unwrap();

        let path = path.to_string_lossy().into_owned();
        let result = store.persist(request(Some(path.clone()), "body", None)).unwrap();
        assert!(result.wrote);
        assert!(!result.conflict);
        assert!(result.mtime_ms.is_some());
        assert_eq!(fs::read_to_string(&path).unwrap(), "body");
        assert!(store.drafts().unwrap().is_empty());
    }

    #[test]
    fn keeps_unsaved_text_in_a_draft_when_the_file_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();
        fs::write(&path, "theirs").unwrap();
        let store = BufferStore::new(MemoryStore::default());

        let result = store.persist(request(Some(path.clone()), "mine", Some(1.0))).unwrap();
        assert!(result.conflict);
        assert!(!result.wrote);
        assert_eq!(fs::read_to_string(&path).unwrap(), "theirs");
        let drafts = store.drafts().unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].record.text, "mine");
        assert_eq!(drafts[0].record.path.as_deref(), Some(path.as_str()));
    }

    #[test]
    fn preserves_eol_and_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.md").to_string_lossy().into_owned();
        let store = BufferStore::new(MemoryStore::default());
        let mut req = request(Some(path.clone()), "a\nb\n", None);
        req.eol = "\r\n".into();
        req.bom = true;
        store.persist(req).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "\u{feff}a\r\nb\r\n");

        let loaded = store.load(&path).unwrap();
        assert_eq!(loaded.text, "a\nb\n");
        assert_eq!(loaded.eol, "\r\n");
        assert!(loaded.bom);
    }

    #[test]
    fn full_path_ids_are_safe_for_conflict_drafts() {
        let store = BufferStore::new(MemoryStore::default());
        let mut req = request(Some("C:\\My Notes\\note.md".into()), "local", None);
        req.id = "C:\\My Notes\\note.md".into();
        req.conflict = true;
        assert!(store.persist(req.clone()).unwrap().conflict);
        assert_eq!(store.drafts().unwrap()[0].record.text, "local");
        store.delete_draft(&req.id).unwrap();
        assert!(store.drafts().unwrap().is_empty());
    }

    #[test]
    fn empty_file_draft_can_be_listed_and_claimed() {
        let store = BufferStore::new(MemoryStore::default());
        let mut req = request(Some("C:\\notes\\a.md".into()), "", None);
        req.id = "old|a".into();
        req.conflict = true;
        req.eol = "\r\n".into();
        req.bom = true;
        store.persist(req).unwrap();
        let draft = store.drafts().unwrap().pop().unwrap();
        assert_eq!(draft.record.text, "");
        assert!(draft.record.bom);
        assert_eq!(draft.record.eol, "\r\n");
        store.claim_draft(&draft, "main|a", "main").unwrap();
        let claimed = store.drafts().unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].record.owner.as_deref(), Some("main"));
        store.delete_draft("main|a").unwrap();
        assert!(store.drafts().unwrap().is_empty());
    }

    #[test]
    fn missing_existing_file_is_kept_as_a_conflict_draft() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moved.md").to_string_lossy().into_owned();
        let store = BufferStore::new(MemoryStore::default());
        let result = store.persist(request(Some(path.clone()), "unsaved", Some(123.0))).unwrap();
        assert!(result.conflict);
        assert!(!Path::new(&path).exists());
        assert_eq!(store.drafts().unwrap()[0].record.text, "unsaved");
    }

    #[test]
    fn rejects_parent_traversal() {
        let store = BufferStore::new(MemoryStore::default());
        let err = store.load("../secret.md").unwrap_err();
        assert!(err.contains(".."));
    }
}
