//! A tiny durable key/value store under `<app data>/state/`. Each key is one file written
//! atomically. Used for drafts, the session, and settings. Keys look like `draft/abc123`.

use crate::fsops::{describe, write_atomic};
use std::fs;
use std::path::{Path, PathBuf};
use crate::app::AppContext;

const EXT: &str = ".kv";


/// Maps a key to a file name. Only a conservative character set is allowed, so keys can
/// never escape the state directory. '/' becomes '~' because '~' isn't allowed in keys.
pub fn key_to_file(key: &str) -> Result<String, String> {
    let valid = !key.is_empty()
        && key.len() <= 200
        && !key.starts_with('/')
        && !key.contains("..")
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if !valid {
        return Err(format!("invalid key: {key:?}"));
    }
    Ok(format!("{}{EXT}", key.replace('/', "~")))
}

pub fn file_to_key(name: &str) -> Option<String> {
    name.strip_suffix(EXT).map(|s| s.replace('~', "/"))
}

fn key_path(dir: &Path, key: &str) -> Result<PathBuf, String> {
    Ok(dir.join(key_to_file(key)?))
}

/// Synchronous access for Rust services that need durable state (for example the personal
/// spellcheck dictionary). Both UI shells use the same helpers.
pub fn read(app: &AppContext, key: &str) -> Result<Option<String>, String> {
    let p = key_path(&app.state_dir()?, key)?;
    match fs::read_to_string(&p) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(describe(&e)),
    }
}

pub fn write(app: &AppContext, key: &str, value: &str) -> Result<(), String> {
    let p = key_path(&app.state_dir()?, key)?;
    write_atomic(&p, value.as_bytes()).map_err(|e| describe(&e))
}

pub fn delete(app: &AppContext, key: &str) -> Result<(), String> {
    let p = key_path(&app.state_dir()?, key)?;
    match fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(describe(&e)),
    }
}

pub fn list(app: &AppContext, prefix: &str) -> Result<Vec<String>, String> {
    let mut keys: Vec<String> = fs::read_dir(app.state_dir()?)
        .map_err(|e| describe(&e))?
        .filter_map(|e| e.ok())
        .filter_map(|e| file_to_key(&e.file_name().to_string_lossy()))
        .filter(|k| k.starts_with(prefix))
        .collect();
    keys.sort();
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        let f = key_to_file("draft/ab-12_x.y").unwrap();
        assert_eq!(f, "draft~ab-12_x.y.kv");
        assert_eq!(file_to_key(&f).unwrap(), "draft/ab-12_x.y");
    }

    #[test]
    fn rejects_unsafe_keys() {
        for bad in ["", "../x", "/abs", "a\\b", "a:b", "a~b", "a b"] {
            assert!(key_to_file(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
