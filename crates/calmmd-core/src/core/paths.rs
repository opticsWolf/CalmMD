//! Cross-platform path helpers shared by the Rust workspace services.
//!
//! They mirror `src/core/paths.ts`: Windows-style paths compare case-insensitively, separators
//! are normalised to `/`, and `is_within` treats the path itself as inside itself.

/// True for `C:/…`, `C:\…` and UNC paths.
pub fn is_windowsish(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\'))
        || path.starts_with("\\\\")
}

pub fn path_key(path: &str) -> String {
    let normalised = path.replace('\\', "/").trim_end_matches('/').to_owned();
    if is_windowsish(path) {
        normalised.to_lowercase()
    } else {
        normalised
    }
}

pub fn is_within(child: &str, parent: &str) -> bool {
    if path_key(child) == path_key(parent) {
        return true;
    }
    let child = path_key(child);
    let parent = path_key(parent);
    child.starts_with(&format!("{parent}/"))
}

pub fn rebase(path: &str, from: &str, to: &str) -> String {
    if path_key(path) == path_key(from) {
        return to.to_owned();
    }
    let normalised = path.replace('\\', "/");
    let prefix = from.replace('\\', "/").trim_end_matches('/').len();
    format!("{}{}", to.trim_end_matches('/'), &normalised[prefix.min(normalised.len())..])
}

pub fn basename(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_owned()
}

pub fn dirname(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        None => String::new(),
        Some(0) => trimmed[..1].to_owned(),
        Some(index) => trimmed[..index].to_owned(),
    }
}

pub fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') || dir.ends_with('\\') {
        format!("{dir}{name}")
    } else {
        let separator = if dir.contains('\\') && !dir.contains('/') { '\\' } else { '/' };
        format!("{dir}{separator}{name}")
    }
}

pub fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".md", ".markdown", ".mdown", ".mkd", ".mdx", ".txt"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// Rejects names common filesystems can't store. Mirrors `validateName` in explorer.ts.
pub fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Enter a name.".into());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("That name is reserved.".into());
    }
    if trimmed.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 0x20) {
        return Err("Names can’t contain \\ / : * ? \" < > |".into());
    }
    if trimmed.ends_with('.') || trimmed.ends_with(' ') {
        return Err("Names can’t end with a dot or space.".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    let stem = lower.split('.').next().unwrap_or("");
    let reserved = stem == "con"
        || stem == "prn"
        || stem == "aux"
        || stem == "nul"
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.chars().last().is_some_and(|c| c.is_ascii_digit()));
    if reserved {
        return Err("That name is reserved on Windows.".into());
    }
    Ok(trimmed.to_owned())
}

/// New files get `.md` unless the user typed an extension.
pub fn with_markdown_ext(name: &str) -> String {
    let trimmed = name.trim();
    let base = basename(trimmed);
    if base.contains('.') && !base.ends_with('.') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_are_case_insensitive() {
        assert_eq!(path_key("C:\\Notes\\A.md"), "c:/notes/a.md");
        assert!(is_within("C:\\Notes\\Sub\\A.md", "c:/notes"));
        assert!(is_within("C:/Notes", "C:\\Notes"));
        assert!(!is_within("C:/Notes2/a.md", "C:/Notes"));
    }

    #[test]
    fn rebases_folders_and_files() {
        assert_eq!(rebase("C:/Notes/Book/Ch 1.md", "C:/Notes/Book", "C:/Notes/New"), "C:/Notes/New/Ch 1.md");
        assert_eq!(rebase("/r/book", "/r/book", "/r/new"), "/r/new");
    }

    #[test]
    fn validates_names_and_extensions() {
        assert_eq!(with_markdown_ext("chapter"), "chapter.md");
        assert_eq!(with_markdown_ext("chapter.txt"), "chapter.txt");
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("CON").is_err());
        assert_eq!(validate_name(" Chapter 1.md ").unwrap(), "Chapter 1.md");
    }
}
