//! Folder operations for the file explorer.

use crate::fsops::describe;
use serde::Serialize;
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Folders we never show: they're huge, generated, or tool-internal.
const SKIP_DIRS: &[&str] = &["node_modules", ".git", ".hg", ".svn", "target", "__pycache__", ".DS_Store"];

/// Natural, case-insensitive order: "Chapter 2" before "Chapter 10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

pub fn list(dir: &Path, show_hidden: bool) -> std::io::Result<Vec<DirEntry>> {
    let mut out: Vec<DirEntry> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // Follow symlinks so linked folders behave like folders.
            let is_dir = fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false);
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            if is_dir && SKIP_DIRS.contains(&name.as_str()) {
                return None;
            }
            Some(DirEntry { path: e.path().to_string_lossy().into_owned(), name, is_dir })
        })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
    Ok(out)
}

pub fn list_dir(path: String, show_hidden: bool) -> Result<Vec<DirEntry>, String> {
    list(Path::new(&path), show_hidden).map_err(|e| describe(&e))
}

pub fn create_file(path: String) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map(|_| ())
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => "A file with that name already exists.".into(),
            _ => describe(&e),
        })
}

pub fn create_dir(path: String) -> Result<(), String> {
    fs::create_dir(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => "A folder with that name already exists.".into(),
        _ => describe(&e),
    })
}

/// Renames without ever replacing an existing file. Case-only renames are allowed on
/// case-insensitive file systems.
pub fn rename(from: &Path, to: &Path) -> Result<(), String> {
    let case_only = from.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase();
    if to.exists() && !case_only {
        return Err("Something with that name already exists.".into());
    }
    fs::rename(from, to).map_err(|e| describe(&e))
}

pub fn rename_path(from: String, to: String) -> Result<(), String> {
    rename(Path::new(&from), Path::new(&to))
}

/// Moves to the OS trash or recycle bin, never a permanent delete.
pub fn trash_path(path: String) -> Result<(), String> {
    trash::delete(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["Chapter 10.md", "chapter 2.md", "Chapter 1.md", "appendix.md", "b", "B2", "b10", "b02"];
        v.sort_by(|a, b| natural_cmp(a, b));
        // "B2" and "b02" compare equal naturally; the byte order breaks the tie deterministically.
        assert_eq!(v, vec!["appendix.md", "b", "B2", "b02", "b10", "Chapter 1.md", "chapter 2.md", "Chapter 10.md"]);
    }

    #[test]
    fn lists_folders_first_and_skips_noise() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("b.md"), "").unwrap();
        fs::write(d.path().join("A.md"), "").unwrap();
        fs::write(d.path().join(".hidden.md"), "").unwrap();
        fs::create_dir(d.path().join("notes")).unwrap();
        fs::create_dir(d.path().join("node_modules")).unwrap();
        let names: Vec<_> = list(d.path(), false).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["notes", "A.md", "b.md"]);
        let with_hidden = list(d.path(), true).unwrap();
        assert!(with_hidden.iter().any(|e| e.name == ".hidden.md"));
    }

    #[test]
    fn rename_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.md");
        let b = d.path().join("b.md");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        assert!(rename(&a, &b).is_err());
        assert_eq!(fs::read_to_string(&b).unwrap(), "b");
        let c = d.path().join("c.md");
        rename(&a, &c).unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), "a");
    }
}
