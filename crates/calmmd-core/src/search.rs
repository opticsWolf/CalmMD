//! Search across a folder: matching lines in Markdown files. Mirrors src/core/search.ts (the
//! browser build and the tests use that one), so keep the rules in sync.

use serde::Serialize;
use std::fs;
use std::path::Path;

const SKIP_DIRS: &[&str] = &["node_modules", ".git", ".hg", ".svn", "target", "__pycache__"];
const EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mdx", "txt"];
const MAX_HITS: usize = 500;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LINE: usize = 240;

#[derive(Serialize, Debug, PartialEq)]
pub struct SearchHit {
    path: String,
    line: usize,
    text: String,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Start offsets (in chars) of matches of `needle` in `line`.
fn find(line: &str, needle: &str, case_sensitive: bool, whole_word: bool) -> Option<usize> {
    let hay: Vec<char> = if case_sensitive { line.chars().collect() } else { line.to_lowercase().chars().collect() };
    let pat: Vec<char> = if case_sensitive { needle.chars().collect() } else { needle.to_lowercase().chars().collect() };
    // Lowercasing can change length for a few characters; fall back to the original line then.
    let orig: Vec<char> = line.chars().collect();
    let chars = if hay.len() == orig.len() { &orig } else { &hay };
    if pat.is_empty() || pat.len() > hay.len() {
        return None;
    }
    let mut i = 0;
    while i + pat.len() <= hay.len() {
        if hay[i..i + pat.len()] == pat[..] {
            let end = i + pat.len();
            let ok = !whole_word
                || ((i == 0 || !is_word(chars[i - 1])) && (end >= chars.len() || !is_word(chars[end])));
            if ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn snippet(line: &str, first: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= MAX_LINE {
        return line.to_string();
    }
    let start = first.saturating_sub(60).min(chars.len() - MAX_LINE);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..start + MAX_LINE]);
    if start + MAX_LINE < chars.len() {
        out.push('…');
    }
    out
}

pub fn search(root: &Path, query: &str, case_sensitive: bool, whole_word: bool) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    if query.trim().is_empty() {
        return hits;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        // Natural order, like the file list ("Chapter 2" before "Chapter 10").
        entries.sort_by(|a, b| crate::tree::natural_cmp(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()));
        let mut subdirs = Vec::new();
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    subdirs.push(path);
                }
                continue;
            }
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if !EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }
            if e.metadata().map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(true) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else { continue };
            for (n, line) in text.lines().enumerate() {
                if let Some(at) = find(line, query, case_sensitive, whole_word) {
                    hits.push(SearchHit { path: path.to_string_lossy().into_owned(), line: n, text: snippet(line, at) });
                    if hits.len() >= MAX_HITS {
                        return hits;
                    }
                }
            }
        }
        // Visit subfolders in name order (the stack pops the last pushed first).
        subdirs.reverse();
        stack.extend(subdirs);
    }
    hits
}

pub fn search_files(root: &str, query: &str, case_sensitive: bool, whole_word: bool) -> Vec<SearchHit> {
    search(Path::new(root), query, case_sensitive, whole_word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lines_in_markdown_files_only() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.md"), "# Calm\nnothing\nso calm here").unwrap();
        fs::write(d.path().join("b.png"), "calm").unwrap();
        fs::create_dir(d.path().join("node_modules")).unwrap();
        fs::write(d.path().join("node_modules").join("x.md"), "calm").unwrap();
        fs::create_dir(d.path().join("sub")).unwrap();
        fs::write(d.path().join("sub").join("c.md"), "CALM").unwrap();
        let hits = search(d.path(), "calm", false, false);
        let lines: Vec<(String, usize)> = hits
            .iter()
            .map(|h| (Path::new(&h.path).file_name().unwrap().to_string_lossy().into_owned(), h.line))
            .collect();
        assert_eq!(lines, vec![("a.md".into(), 0), ("a.md".into(), 2), ("c.md".into(), 0)]);
        assert!(search(d.path(), "calm", true, false).iter().all(|h| !h.path.ends_with("c.md")));
    }

    #[test]
    fn whole_words_and_unicode() {
        assert_eq!(find("cat catalog cat", "cat", false, true), Some(0));
        assert_eq!(find("catalog cat", "cat", false, true), Some(8));
        assert_eq!(find("cafés café", "café", false, true), Some(6));
        assert_eq!(find("nothing", "", false, false), None);
    }

    #[test]
    fn long_lines_keep_the_match() {
        let line = format!("{}NEEDLE{}", "x".repeat(500), "y".repeat(500));
        let s = snippet(&line, 500);
        assert!(s.contains("NEEDLE") && s.starts_with('…') && s.ends_with('…'));
    }
}
