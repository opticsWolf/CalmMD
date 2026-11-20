//! Export preference and cover normalization shared by MCP tools.
//!
//! Keeping this in Rust means path resolution, live-text reads and option validation happen before
//! the remaining webview render call. The webview still owns DOM/Mermaid rendering and the final
//! document assembly, so export output stays identical.

use serde_json::{json, Value};

fn str_field(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn bool_field(value: Option<&Value>, key: &str, default: bool) -> bool {
    value.and_then(|v| v.get(key)).and_then(Value::as_bool).unwrap_or(default)
}

fn one_of(value: Option<&Value>, key: &str, allowed: &[&str], default: &str) -> String {
    str_field(value, key)
        .filter(|v| allowed.contains(&v.as_str()))
        .unwrap_or_else(|| default.to_owned())
}

/// Mirrors `parseExportPrefs` in `src/app/exporter.ts`.
pub fn resolve_prefs(raw: Option<&Value>) -> Value {
    let pdf = raw.and_then(|v| v.get("pdf"));
    json!({
        "pdf": {
            "pageSize": one_of(pdf, "pageSize", &["A4", "Letter", "A5", "Legal"], "A4"),
            "margins": one_of(pdf, "margins", &["narrow", "normal", "wide"], "normal"),
            "header": bool_field(pdf, "header", true),
            "pageNumbers": bool_field(pdf, "pageNumbers", true),
            "breakBeforeH1": bool_field(pdf, "breakBeforeH1", false),
            "template": one_of(pdf, "template", &["standard", "report", "academic", "minimal"], "standard"),
        },
        "toc": bool_field(raw, "toc", false),
        "embedImages": bool_field(raw, "embedImages", true),
        "theme": str_field(raw, "theme"),
        "cover": bool_field(raw, "cover", false),
        "coverAuthor": str_field(raw, "coverAuthor")
            .map(|s| s.chars().take(200).collect::<String>())
            .unwrap_or_default(),
    })
}

fn front_matter(markdown: &str) -> Vec<(String, String)> {
    let mut lines = markdown.lines();
    let Some(first) = lines.next() else { return Vec::new() };
    if first.trim_end() != "---" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            break;
        }
        let Some((key, value)) = trimmed.split_once(':') else { continue };
        let key = key.trim();
        if key.is_empty() || !key.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        out.push((key.to_ascii_lowercase(), value.to_owned()));
    }
    out
}

fn first_h1(markdown: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let start = if lines.first().is_some_and(|line| line.trim_end() == "---") {
        lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| line.trim_end() == "---" || line.trim_end() == "...")
            .map(|(i, _)| i + 1)
            .unwrap_or(0)
    } else {
        0
    };
    for line in lines.iter().skip(start) {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("# ") else { continue };
        let title = rest.trim_end().trim_end_matches('#').trim().replace(['*', '_', '`'], "");
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

/// Mirrors `coverInfo` in `src/app/mcpBridge.ts`. Date defaults are left to the webview so the
/// user's locale still decides today's format.
pub fn resolve_cover(markdown: &str, fallback_title: &str, explicit: Option<&Value>, prefs: &Value) -> Value {
    let wants = prefs.get("cover").and_then(Value::as_bool).unwrap_or(false);
    if !wants && explicit.is_none() {
        return Value::Null;
    }
    let fields = front_matter(markdown);
    let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let author_default = str_field(Some(prefs), "coverAuthor").unwrap_or_default();
    let mut cover = json!({
        "title": explicit.and_then(|v| str_field(Some(v), "title"))
            .or_else(|| get("title"))
            .or_else(|| first_h1(markdown))
            .unwrap_or_else(|| fallback_title.to_owned()),
        "subtitle": explicit.and_then(|v| str_field(Some(v), "subtitle"))
            .or_else(|| get("subtitle"))
            .or_else(|| get("description"))
            .unwrap_or_default(),
        "author": explicit.and_then(|v| str_field(Some(v), "author"))
            .or_else(|| get("author"))
            .unwrap_or(author_default),
    });
    if let Some(date) = explicit.and_then(|v| str_field(Some(v), "date")).or_else(|| get("date")) {
        cover["date"] = json!(date);
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_fill_defaults_and_reject_invalid_values() {
        let prefs = resolve_prefs(Some(&json!({
            "pdf": { "pageSize": "invalid", "margins": "wide", "template": "report", "header": false },
            "toc": true,
            "coverAuthor": "Ada"
        })));
        assert_eq!(prefs["pdf"]["pageSize"], "A4");
        assert_eq!(prefs["pdf"]["margins"], "wide");
        assert_eq!(prefs["pdf"]["template"], "report");
        assert_eq!(prefs["pdf"]["header"], false);
        assert_eq!(prefs["pdf"]["pageNumbers"], true);
        assert_eq!(prefs["toc"], true);
        assert_eq!(prefs["embedImages"], true);
        assert_eq!(prefs["coverAuthor"], "Ada");
    }

    #[test]
    fn cover_uses_front_matter_then_heading_then_filename() {
        let cover = resolve_cover("# Heading\n\ntext", "file", None, &json!({ "cover": true }));
        assert_eq!(cover["title"], "Heading");
        assert_eq!(cover["subtitle"], "");

        let cover = resolve_cover(
            "---\ntitle: Note\ndescription: Summary\ndate: 2025-01-02\n---\n# Heading",
            "file",
            None,
            &json!({ "cover": true }),
        );
        assert_eq!(cover["title"], "Note");
        assert_eq!(cover["subtitle"], "Summary");
        assert_eq!(cover["date"], "2025-01-02");
    }

    #[test]
    fn explicit_cover_fields_win() {
        let cover = resolve_cover(
            "# Heading",
            "file",
            Some(&json!({ "title": "Chosen", "author": "Ada" })),
            &json!({ "cover": false }),
        );
        assert_eq!(cover["title"], "Chosen");
        assert_eq!(cover["author"], "Ada");
    }
}
