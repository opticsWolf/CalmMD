//! Single-instance lock and launch-path queue.
//!
//! GPUI has no built-in single-instance coordinator. A lock file in the data directory gives one
//! owner; later launches append their files to a small JSON queue that the owner drains.

use fs2::FileExt as _;
use serde_json::Value;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Acquires the process-wide lock, or returns `None` when another CalmMD owns it.
    pub fn acquire(data_dir: &Path) -> Option<Self> {
        fs::create_dir_all(data_dir).ok()?;
        let file = File::create(data_dir.join("instance.lock")).ok()?;
        file.try_lock_exclusive().ok()?;
        Some(Self { _file: file })
    }
}

fn queue_path(data_dir: &Path) -> PathBuf {
    data_dir.join("launch-queue.json")
}

/// Appends launch paths for the running instance to pick up.
pub fn queue_launch_paths(data_dir: &Path, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    let _ = fs::create_dir_all(data_dir);
    let path = queue_path(data_dir);
    let mut all: Vec<String> = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default();
    for item in paths {
        if !all.contains(&item) {
            all.push(item);
        }
    }
    if let Ok(text) = serde_json::to_string(&all) {
        let _ = fs::write(path, text);
    }
}

/// Removes and returns queued launch paths.
pub fn take_launch_paths(data_dir: &Path) -> Vec<String> {
    let path = queue_path(data_dir);
    let Ok(text) = fs::read_to_string(&path) else { return Vec::new() };
    let _ = fs::remove_file(&path);
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

/// Command-line paths worth opening: existing Markdown files and folders.
pub fn launch_paths(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|arg| {
            let path = Path::new(arg);
            path.is_dir() || path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .filter_map(|arg| fs::canonicalize(&arg).ok())
        .map(|path| {
            let text = path.to_string_lossy().into_owned();
            text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queues_and_drains_paths() {
        let dir = tempfile::tempdir().unwrap();
        queue_launch_paths(dir.path(), vec!["a.md".into()]);
        queue_launch_paths(dir.path(), vec!["a.md".into(), "b.md".into()]);
        assert_eq!(take_launch_paths(dir.path()), vec!["a.md", "b.md"]);
        assert!(take_launch_paths(dir.path()).is_empty());
    }

    #[test]
    fn launch_paths_accepts_folders_and_markdown_only() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("note.md");
        std::fs::write(&md, "# Note").unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, "notes").unwrap();
        let result = launch_paths([
            "calmmd-gpui".into(),
            md.to_string_lossy().into_owned(),
            txt.to_string_lossy().into_owned(),
            dir.path().to_string_lossy().into_owned(),
        ]);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|p| p.ends_with("note.md")));
    }
}
