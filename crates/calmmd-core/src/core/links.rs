//! Link rewriting used when a note moves. Port of the relevant part of `src/core/noteLinks.ts`.

use super::paths::{basename, dirname, is_markdown_path, path_key};

fn decode_link_path(url: &str) -> String {
    let bytes = url.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&url[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn markdown_url(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.replace('\\', "/").chars() {
        if matches!(ch, ' ' | '(' | ')' | '<' | '>' | '#' | '?' | '%' | '[' | ']') {
            out.push('%');
            out.extend(format!("{:02X}", ch as u32).chars());
        } else {
            out.push(ch);
        }
    }
    out
}

fn segments(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .split('/')
        .map(str::to_owned)
        .collect()
}

pub fn relative_path(from_dir: &str, target: &str) -> Option<String> {
    let windows = is_windowsish(from_dir) || is_windowsish(target);
    let a = segments(from_dir);
    let b = segments(target);
    let equal = |x: &str, y: &str| if windows { x.eq_ignore_ascii_case(y) } else { x == y };
    if windows && (!a.is_empty() && !b.is_empty() && !equal(&a[0], &b[0])) {
        return None;
    }
    let mut common = 0;
    while common < a.len() && common < b.len() && equal(&a[common], &b[common]) {
        common += 1;
    }
    let up = a[common..].iter().filter(|part| !part.is_empty()).map(|_| "..".to_owned());
    let out: Vec<String> = up.chain(b[common..].iter().cloned()).collect();
    Some(if out.is_empty() { ".".into() } else { out.join("/") })
}

fn is_windowsish(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\'))
        || path.starts_with("\\\\")
}

pub fn resolve_link(url: &str, doc_dir: &str) -> Option<String> {
    if url.starts_with('#') || url.starts_with("//") || url.starts_with("data:") {
        return None;
    }
    let scheme = url.split_once(':').map(|(head, _)| head).unwrap_or("");
    if !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !(scheme.len() == 1 && url[1..].starts_with(":/"))
    {
        return None;
    }
    let path = decode_link_path(url);
    if path.starts_with('/') || is_windowsish(&path) {
        return Some(path);
    }
    let mut parts = segments(doc_dir);
    for segment in path.replace('\\', "/").split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            other => parts.push(other.to_owned()),
        }
    }
    let joined = parts.join("/");
    Some(if doc_dir.contains('\\') && !doc_dir.contains('/') {
        joined.replace('/', "\\")
    } else {
        joined
    })
}

fn stem(path: &str) -> String {
    let name = basename(path);
    match name.rfind('.') {
        Some(index) if index > 0 => name[..index].to_owned(),
        _ => name,
    }
}

fn wiki_stem(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".md") {
        &name[..name.len() - 3]
    } else if lower.ends_with(".markdown") {
        &name[..name.len() - 9]
    } else {
        name
    }
}

fn resolve_wiki(name: &str, from_doc: &str, files: &[String]) -> Option<String> {
    let want = wiki_stem(name).to_ascii_lowercase();
    let has_path = want.contains('/');
    let candidates: Vec<&String> = files
        .iter()
        .filter(|file| {
            if !is_markdown_path(file) {
                return false;
            }
            let normal = file.replace('\\', "/").to_ascii_lowercase();
            let without_ext = normal
                .strip_suffix(".markdown")
                .or_else(|| normal.strip_suffix(".md"))
                .unwrap_or(&normal);
            if has_path {
                without_ext.ends_with(&format!("/{want}")) || without_ext == want
            } else {
                stem(file).to_ascii_lowercase() == want
            }
        })
        .collect();
    let here = dirname(from_doc);
    let depth = |path: &str| path.replace('\\', "/").split('/').count();
    let score = |file: &str| {
        let parent = dirname(file);
        if path_key(&parent) == path_key(&here) {
            -1i32
        } else if super::paths::is_within(file, &here) {
            depth(file) as i32 - depth(&here) as i32
        } else {
            1000 + depth(file) as i32
        }
    };
    candidates
        .into_iter()
        .min_by(|a, b| score(a).cmp(&score(b)).then_with(|| a.len().cmp(&b.len())).then_with(|| a.cmp(b)))
        .cloned()
}

#[derive(Debug, Clone)]
struct Edit {
    from: usize,
    to: usize,
    text: String,
}

fn fence_len(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let first = *trimmed.as_bytes().first()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|byte| *byte == first).count();
    (len >= 3).then_some((first, len))
}

fn mask_inline_code(line: &str) -> Vec<u8> {
    let bytes = line.as_bytes();
    let mut masked = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let mut run = 1;
            while i + run < bytes.len() && bytes[i + run] == b'`' {
                run += 1;
            }
            let marker = &bytes[i..i + run];
            let mut end = i + run;
            while end + run <= bytes.len() && &bytes[end..end + run] != marker {
                end += 1;
            }
            if end + run <= bytes.len() {
                for byte in &mut masked[i..end + run] {
                    *byte = b' ';
                }
                i = end + run;
                continue;
            }
            i += run;
        } else {
            i += 1;
        }
    }
    masked
}

/// Rewrites links in `text` (the note at `from_doc`) that point at `from` to point at `to`.
pub fn rewrite_links(text: &str, from_doc: &str, from: &str, to: &str, files: &[String]) -> String {
    let mut edits = Vec::new();
    // Callers collect the file list after the move, so it holds `to` and not `from`. Keep the old
    // path resolvable or wiki links cannot be recognised.
    let wiki_files: Vec<String> = if files.iter().any(|file| path_key(file) == path_key(from)) {
        files.to_vec()
    } else {
        files.iter().cloned().chain(std::iter::once(from.to_owned())).collect()
    };
    let mut pos = 0usize;
    let mut fence: Option<(u8, usize)> = None;
    let dir = dirname(from_doc);

    for line in text.split('\n') {
        let start = pos;
        pos += line.len() + 1;
        let found_fence = fence_len(line);
        if let Some((char, len)) = fence {
            if let Some((next_char, next_len)) = found_fence {
                if next_char == char && next_len >= len && line.trim_end().bytes().all(|b| b == char || b == b'`' || b == b'~') {
                    fence = None;
                }
            }
            continue;
        }
        if let Some(value) = found_fence {
            fence = Some(value);
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let masked = mask_inline_code(line);
        let masked_str = String::from_utf8_lossy(&masked);

        // Wiki links: [[Title]], [[Title#Heading|alias]]
        let mut cursor = 0;
        while let Some(relative) = masked_str[cursor..].find("[[") {
            let open = cursor + relative;
            let Some(close_rel) = masked_str[open + 2..].find("]]") else { break };
            let close = open + 2 + close_rel;
            let inner = &line[open + 2..close];
            let (left, alias) = inner.split_once('|').unwrap_or((inner, ""));
            let (name, heading) = left.split_once('#').unwrap_or((left, ""));
            let name = name.trim();
            if !name.is_empty() {
                if let Some(target) = resolve_wiki(name, from_doc, &wiki_files) {
                    if path_key(&target) == path_key(from) {
                        let mut next = format!("[[{}", stem(to));
                        if !heading.is_empty() {
                            next.push('#');
                            next.push_str(heading.trim());
                        }
                        if !alias.is_empty() {
                            next.push('|');
                            next.push_str(alias.trim());
                        }
                        next.push_str("]]");
                        edits.push(Edit { from: start + open, to: start + close + 2, text: next });
                    }
                }
            }
            cursor = close + 2;
        }

        // Standard Markdown links: [label](destination "title")
        cursor = 0;
        while let Some(relative) = masked_str[cursor..].find("](") {
            let open = cursor + relative;
            if open > 0 && line.as_bytes().get(open - 1) == Some(&b'!') {
                cursor = open + 2;
                continue;
            }
            let Some(close_rel) = masked_str[open + 2..].find(')') else { break };
            let close = open + 2 + close_rel;
            let inside = &line[open + 2..close];
            let (raw, suffix) = if let Some(rest) = inside.strip_prefix('<') {
                match rest.find('>') {
                    Some(end) => (&rest[..end], &rest[end + 1..]),
                    None => (inside, ""),
                }
            } else {
                match inside.find(|c: char| c.is_whitespace() || c == ')') {
                    Some(end) => (&inside[..end], &inside[end..]),
                    None => (inside, ""),
                }
            };
            let (path_part, anchor) = raw.split_once('#').unwrap_or((raw, ""));
            if let Some(target) = resolve_link(path_part, &dir) {
                if path_key(&target) == path_key(from) {
                    if let Some(rel) = relative_path(&dir, to) {
                        let angled = inside.starts_with('<');
                        let url = markdown_url(&rel);
                        let destination = if angled {
                            format!("<{}{}>", url, if anchor.is_empty() { String::new() } else { format!("#{anchor}") })
                        } else {
                            format!("{}{}", url, if anchor.is_empty() { String::new() } else { format!("#{anchor}") })
                        };
                        let label = &line[..open];
                        let bracket = label.rfind('[').unwrap_or(0);
                        let label_start = bracket + 1;
                        edits.push(Edit {
                            from: start + bracket,
                            to: start + close + 1,
                            text: format!("[{}]({}{})", &line[label_start..open], destination, suffix),
                        });
                    }
                }
            }
            cursor = close + 1;
        }
    }

    edits.sort_by(|a, b| b.from.cmp(&a.from));
    let mut out = text.to_owned();
    for edit in edits {
        if edit.from <= edit.to && edit.to <= out.len() {
            out.replace_range(edit.from..edit.to, &edit.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_wiki_links_when_the_listing_is_post_rename() {
        // The explorer renames first, so the file list no longer contains the old name.
        let renamed = vec!["C:/notes/a.md".to_string(), "C:/notes/plan.md".to_string()];
        let text = "See [[b]] and [b](b.md).\n";
        let out = rewrite_links(text, "C:/notes/a.md", "C:/notes/b.md", "C:/notes/plan.md", &renamed);
        assert_eq!(out, "See [[plan]] and [b](plan.md).\n");
    }

    #[test]
    fn rewrites_relative_links_and_wiki_links() {
        let root = "C:/notes";
        let files = vec![
            format!("{root}/from.md"),
            format!("{root}/old.md"),
            format!("{root}/new.md"),
        ];
        let text = "# Note\n\n[old](old.md) and [anchor](old.md#Section) and [[old#Section|Alias]].\n\n`[old](old.md)`\n\n```\n[old](old.md)\n```\n";
        let result = rewrite_links(text, &format!("{root}/from.md"), &format!("{root}/old.md"), &format!("{root}/sub/new.md"), &files);
        assert!(result.contains("[old](sub/new.md)"));
        assert!(result.contains("[anchor](sub/new.md#Section)"));
        assert!(result.contains("[[new#Section|Alias]]"));
        assert!(result.contains("`[old](old.md)`"));
        assert!(result.contains("```\n[old](old.md)\n```"));
    }

    #[test]
    fn rewrites_the_smoke_note_exactly() {
        let root = "D:/Daten/Downloads/tmp/calmmd-mcp-rust";
        let files = vec![
            format!("{root}/note-01.md"),
            format!("{root}/note-05.md"),
            format!("{root}/Areas/garden-plan.md"),
        ];
        let text = "See also [garden note](note-05.md).
";
        let result = rewrite_links(text, &format!("{root}/note-01.md"), &format!("{root}/note-05.md"), &format!("{root}/Areas/garden-plan.md"), &files);
        assert_eq!(result, "See also [garden note](Areas/garden-plan.md).
");
    }

    #[test]
    fn relative_paths_handle_windows_and_parent_folders() {
        assert_eq!(relative_path("C:/notes/sub", "C:/notes/Areas/a.md").unwrap(), "../Areas/a.md");
        assert_eq!(relative_path("/notes/sub", "/notes/sub/a.md").unwrap(), "a.md");
        assert_eq!(markdown_url("a b(c).md"), "a%20b%28c%29.md");
    }
}
