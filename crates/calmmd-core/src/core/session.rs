//! Rust-owned session and Space records.
//!
//! The webview still builds the snapshot (it owns CodeMirror cursors and the sidebar), but Rust
//! validates and stores it. The KV keys and JSON shapes stay compatible with the old TypeScript
//! store so upgrading CalmMD does not discard saved sessions.

use super::history::HistoryStore;
use crate::kv;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::app::AppContext;

pub const SESSION_KEY: &str = "session";
pub const SPACE_PREFIX: &str = "space/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTabDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    pub cursor: usize,
    pub top_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarDto {
    pub open: bool,
    pub view: String,
    pub width: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    pub v: u8,
    pub tabs: Vec<SessionTabDto>,
    pub active: usize,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_roots: Vec<String>,
    #[serde(default)]
    pub expanded: Vec<String>,
    pub sidebar: SidebarDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceDto {
    pub v: u8,
    pub id: String,
    pub name: String,
    pub created_at: u128,
    pub last_opened: u128,
    pub session: SessionDto,
}

impl Default for SessionDto {
    fn default() -> Self {
        Self {
            v: 1,
            tabs: Vec::new(),
            active: 0,
            root: None,
            extra_roots: Vec::new(),
            expanded: Vec::new(),
            sidebar: SidebarDto {
                open: true,
                view: "files".into(),
                width: 248,
            },
            view_mode: None,
        }
    }
}

fn string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn non_negative(value: Option<&Value>, fallback: usize) -> usize {
    value
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n.floor() as usize)
        .unwrap_or(fallback)
}

fn session_from_value(value: Option<&Value>) -> SessionDto {
    let Some(value) = value else { return SessionDto::default() };
    let tabs: Vec<SessionTabDto> = value
        .get("tabs")
        .and_then(Value::as_array)
        .map(|items: &Vec<Value>| {
            items
                .iter()
                .filter_map(|tab| {
                    let path = string(tab.get("path"));
                    let draft_id = string(tab.get("draftId"));
                    if path.is_none() && draft_id.is_none() {
                        return None;
                    }
                    Some(SessionTabDto {
                        path,
                        draft_id,
                        cursor: non_negative(tab.get("cursor"), 0),
                        top_line: non_negative(tab.get("topLine"), 0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let active = non_negative(value.get("active"), 0).min(tabs.len().saturating_sub(1));
    let sidebar = value.get("sidebar");
    let view = string(sidebar.and_then(|s| s.get("view"))).unwrap_or_else(|| "files".into());
    let view = if matches!(view.as_str(), "files" | "outline" | "search" | "history") {
        view
    } else {
        "files".into()
    };
    let width = non_negative(sidebar.and_then(|s| s.get("width")), 248).clamp(180, 480);
    let view_mode = string(value.get("viewMode")).filter(|mode| matches!(mode.as_str(), "source" | "live" | "split" | "preview"));
    SessionDto {
        v: 1,
        tabs,
        active,
        root: string(value.get("root")),
        extra_roots: value.get("extraRoots").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|item| string(Some(item))).collect(),
        expanded: value
            .get("expanded")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        sidebar: SidebarDto {
            open: sidebar.and_then(|s| s.get("open")).and_then(Value::as_bool).unwrap_or(true),
            view,
            width,
        },
        view_mode,
    }
}

fn parse_session(raw: Option<&str>) -> SessionDto {
    let Some(raw) = raw else { return SessionDto::default() };
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return SessionDto::default() };
    session_from_value(Some(&value))
}

fn space_from_value(value: &Value) -> Option<SpaceDto> {
    if value.get("v").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let id = string(value.get("id"))?;
    let name = string(value.get("name")).unwrap_or_else(|| "Untitled Space".into());
    Some(SpaceDto {
        v: 1,
        id,
        name,
        created_at: non_negative(value.get("createdAt"), 0) as u128,
        last_opened: non_negative(value.get("lastOpened"), 0) as u128,
        session: session_from_value(value.get("session")),
    })
}

fn parse_space(raw: Option<&str>) -> Option<SpaceDto> {
    let raw = raw?;
    let value = serde_json::from_str::<Value>(raw).ok()?;
    space_from_value(&value)
}

fn validate_key(key: &str) -> Result<(), String> {
    if key == SESSION_KEY {
        return Ok(());
    }
    if let Some(id) = key.strip_prefix(SPACE_PREFIX) {
        if !id.is_empty()
            && id.len() <= 100
            && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            kv::key_to_file(key)?;
            return Ok(());
        }
    }
    Err(format!("invalid session key: {key:?}"))
}

/// Pure session store, generic over the same KV interface used by drafts and history.
pub struct SessionStore<S: HistoryStore> {
    store: S,
}

impl<S: HistoryStore> SessionStore<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn session(&self, key: &str) -> Result<Option<SessionDto>, String> {
        validate_key(key)?;
        Ok(self.store.get(key)?.as_deref().map(|raw| parse_session(Some(raw))))
    }

    pub fn set_session(&self, key: &str, session: &SessionDto) -> Result<(), String> {
        validate_key(key)?;
        let normalised = parse_session(Some(&serde_json::to_string(session).map_err(|e| e.to_string())?));
        self.store.set(key, &serde_json::to_string(&normalised).map_err(|e| e.to_string())?)
    }

    pub fn space(&self, id: &str) -> Result<Option<SpaceDto>, String> {
        let key = format!("{SPACE_PREFIX}{id}");
        validate_key(&key)?;
        let raw = self.store.get(&key)?;
        Ok(parse_space(raw.as_deref()))
    }

    pub fn save_space(&self, space: &SpaceDto) -> Result<(), String> {
        let key = format!("{SPACE_PREFIX}{}", space.id);
        validate_key(&key)?;
        let name = space.name.trim().to_owned();
        let record = SpaceDto {
            v: 1,
            id: space.id.clone(),
            name: if name.is_empty() { "Untitled Space".into() } else { name },
            created_at: space.created_at,
            last_opened: space.last_opened,
            session: parse_session(Some(&serde_json::to_string(&space.session).map_err(|e| e.to_string())?)),
        };
        self.store.set(&key, &serde_json::to_string(&record).map_err(|e| e.to_string())?)
    }

    pub fn remove_space(&self, id: &str) -> Result<(), String> {
        self.store.delete(&format!("{SPACE_PREFIX}{id}"))
    }

    pub fn spaces(&self) -> Result<Vec<SpaceDto>, String> {
        let mut out = self
            .store
            .list(SPACE_PREFIX)?
            .into_iter()
            .filter_map(|key| self.store.get(&key).ok().flatten())
            .filter_map(|raw| parse_space(Some(&raw)))
            .collect::<Vec<_>>();
        out.sort_by(|a, b| b.last_opened.cmp(&a.last_opened));
        Ok(out)
    }
}


pub fn app_store(app: &AppContext) -> SessionStore<crate::core::history::AppHistoryStore> {
    SessionStore::new(crate::core::history::AppHistoryStore::new(app.clone()))
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

    #[test]
    fn parses_legacy_session_json_defensively() {
        let store = SessionStore::new(MemoryStore::default());
        let raw = r#"{"v":1,"tabs":[{"path":"/a.md","cursor":-4,"topLine":3},{"draftId":"x"}],"active":99,"root":"/notes","sidebar":{"view":"nope","width":9999}}"#;
        store.store.set(SESSION_KEY, raw).unwrap();
        let session = store.session(SESSION_KEY).unwrap().unwrap();
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].cursor, 0);
        assert_eq!(session.tabs[1].path, None);
        assert_eq!(session.active, 1);
        assert_eq!(session.sidebar.view, "files");
        assert_eq!(session.sidebar.width, 480);
    }

    #[test]
    fn round_trips_a_session() {
        let store = SessionStore::new(MemoryStore::default());
        let mut session = SessionDto::default();
        session.tabs.push(SessionTabDto {
            path: Some("C:/notes/a.md".into()),
            draft_id: None,
            cursor: 12,
            top_line: 3,
        });
        session.active = 0;
        session.root = Some("C:/notes".into());
        session.sidebar.width = 300;
        store.set_session(SESSION_KEY, &session).unwrap();
        assert_eq!(store.session(SESSION_KEY).unwrap().unwrap(), session);
    }

    #[test]
    fn lists_spaces_newest_first() {
        let store = SessionStore::new(MemoryStore::default());
        let make = |id: &str, opened: u128| SpaceDto {
            v: 1,
            id: id.into(),
            name: id.into(),
            created_at: 1,
            last_opened: opened,
            session: SessionDto::default(),
        };
        store.save_space(&make("old", 10)).unwrap();
        store.save_space(&make("new", 20)).unwrap();
        let spaces = store.spaces().unwrap();
        assert_eq!(spaces.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        store.remove_space("old").unwrap();
        assert_eq!(store.spaces().unwrap().len(), 1);
    }

    #[test]
    fn rejects_unsafe_keys() {
        let store = SessionStore::new(MemoryStore::default());
        for key in ["../../bad", "space/bad/id", "space/bad.id", "other"] {
            assert!(store.session(key).is_err(), "{key}");
        }
    }
}
