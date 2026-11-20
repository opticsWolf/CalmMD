//! Browser-free export document construction.
//!
//! This is the Rust counterpart of `buildBody`/`buildDocument`. It renders Markdown with Mordant,
//! embeds local images and offline CSS/font assets, then assembles the standalone HTML shell. MCP
//! exports therefore do not need a running webview.

use super::{assets::ExportAssets, assemble::assemble_from_value, takumi::TakumiPdfOptions};
use crate::core::{
    dto::MarkdownRenderOptions,
    markdown::render_html,
    paths::{basename, dirname, join_path},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, time::{SystemTime, UNIX_EPOCH}};

const MAX_EMBEDDED_IMAGE: u64 = 24 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfPrefs {
    page_size: String,
    margins: String,
    header: bool,
    page_numbers: bool,
    break_before_h1: bool,
    template: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportPrefs {
    pdf: PdfPrefs,
    toc: bool,
    embed_images: bool,
    theme: Option<String>,
    cover: bool,
    cover_author: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Cover {
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    date: String,
}

/// Builds a standalone HTML export from live Markdown text. `path` anchors relative images and
/// supplies the document title; `for_pdf` adds print CSS and the Paged.js comparison polyfill.
pub fn build_html(text: &str, path: &str, prefs: &Value, cover: &Value, for_pdf: bool) -> Result<String, String> {
    let prefs: ExportPrefs = serde_json::from_value(prefs.clone()).map_err(|e| format!("Invalid export preferences: {e}"))?;
    let cover: Cover = if cover.is_null() {
        Cover::default()
    } else {
        serde_json::from_value(cover.clone()).map_err(|e| format!("Invalid cover: {e}"))?
    };

    let theme = prefs.theme.clone().unwrap_or_else(|| "paper".to_owned());
    let rendered = render_html(
        text,
        &MarkdownRenderOptions {
            highlight_code: true,
            theme: theme.clone(),
            ..MarkdownRenderOptions::default()
        },
    )
    .map_err(|e| format!("Markdown render failed: {e}"))?;

    let mut body = strip_line_anchors(&rendered.html);
    body = disable_task_boxes(&body);
    body = flatten_wiki_links(&body);
    if prefs.embed_images {
        body = embed_images(&body, path)?;
    }
    body = strip_attribute(&body, "loading");
    body = embed_cards_to_links(&body);
    if prefs.toc {
        body = format!("{}\n{}", table_of_contents(&body), body);
    }
    if prefs.cover {
        body = format!("{}\n{}", cover_html(&cover), body);
    }

    let has_math = body.contains("class=\"katex");
    let assets = ExportAssets::load(for_pdf, has_math);
    let request = json!({
        "body": body,
        "title": basename(path).trim_end_matches(".md"),
        "theme": theme,
        "userCss": Value::Null,
        "baseCss": assets.base_css,
        "fontCss": assets.font_css,
        "katexCss": assets.katex_css,
        "pagedJs": assets.paged_js,
        "pdf": if for_pdf { json!(prefs.pdf) } else { Value::Null },
    });
    let result = assemble_from_value(request).map_err(|e| format!("Export assembly failed: {e}"))?;
    result.get("html").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| "Export assembly returned no HTML".into())
}

/// Writes a standalone HTML export and returns a small tool result.
pub fn export_html(path: &str, out: &str, text: &str, prefs: &Value, cover: &Value) -> Result<Value, String> {
    let html = build_html(text, path, prefs, cover, false)?;
    write_text(out, &html)?;
    Ok(json!({ "path": out, "bytes": html.len() }))
}

/// Writes a Takumi PDF export and returns a small tool result.
pub fn export_pdf(path: &str, out: &str, text: &str, prefs: &Value, cover: &Value) -> Result<Value, String> {
    let html = build_html(text, path, prefs, cover, true)?;
    let title = basename(path).trim_end_matches(".md").to_owned();
    let options = takumi_options(&title, prefs, cover)?;
    super::takumi::render_pdf_file(&html, out, &options)?;
    let bytes = fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    Ok(json!({ "path": out, "bytes": bytes }))
}

fn write_text(out: &str, text: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    fs::write(out, text).map_err(|e| e.to_string())
}

fn takumi_options(title: &str, prefs: &Value, cover: &Value) -> Result<TakumiPdfOptions, String> {
    let prefs: ExportPrefs = serde_json::from_value(prefs.clone()).map_err(|e| e.to_string())?;
    let wants_cover = prefs.cover || cover.is_object();
    let small = "font-family:Inter Variable,sans-serif;font-size:9px;color:#8a8f99;";
    let page = "<span class=\"pageNumber\"></span>";
    let pages = "<span class=\"totalPages\"></span>";
    let report_header = if prefs.pdf.header && !title.is_empty() && prefs.pdf.template == "report" {
        Some(format!("<div style=\"{small}display:flex;justify-content:space-between;padding:0 76px\">{}<span>{page}</span></div>", escape_html(title)))
    } else {
        None
    };
    let header = if !prefs.pdf.header || title.is_empty() || prefs.pdf.template == "minimal" {
        None
    } else if let Some(report) = report_header {
        Some(report)
    } else {
        Some(format!("<div style=\"{small}text-align:center\">{}</div>", escape_html(title)))
    };
    let footer = if !prefs.pdf.page_numbers {
        None
    } else {
        Some(match prefs.pdf.template.as_str() {
            "report" => format!("<div style=\"{small}text-align:right;padding-right:76px\">Page {page} of {pages}</div>"),
            "minimal" => format!("<div style=\"{small}text-align:right;padding-right:76px\">{page}</div>"),
            _ => format!("<div style=\"{small}text-align:center\">{page} / {pages}</div>"),
        })
    };
    let margin = match prefs.pdf.margins.as_str() {
        "narrow" => (53.0, 53.0, 60.0, 53.0),
        "wide" => (113.0, 106.0, 121.0, 106.0),
        _ => (83.0, 76.0, 91.0, 76.0),
    };
    let size = match prefs.pdf.page_size.as_str() {
        "Letter" => "letter",
        "A5" => "a5",
        "Legal" => "legal",
        _ => "a4",
    };
    Ok(TakumiPdfOptions {
        size: size.to_owned(),
        margin: super::takumi::TakumiMargins { top: margin.0, right: margin.1, bottom: margin.2, left: margin.3 },
        header,
        footer,
        pages: super::takumi::TakumiPageRules {
            first: super::takumi::TakumiPageOverride {
                header: Some(false),
                footer: if wants_cover { Some(false) } else { None },
            },
        },
        outline: true,
        metadata: super::takumi::TakumiMetadata { title: Some(title.to_owned()), creator: Some("CalmMD".into()) },
        lang: "en".into(),
        heading_numbers: prefs.pdf.template == "academic",
        font_weight: 350.0,
    })
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn map_tags(html: &str, mut map: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let tag = &after[1..end];
        match map(tag) {
            Some(replacement) => out.push_str(&replacement),
            None => {
                out.push('<');
                out.push_str(tag);
                out.push('>');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn strip_line_anchors(html: &str) -> String {
    let mut out = html.to_owned();
    for name in ["data-line-start", "data-line-end", "data-cursor", "data-task-line"] {
        out = strip_attribute(&out, name);
    }
    out
}

fn strip_attribute(html: &str, name: &str) -> String {
    let mut out = html.to_owned();
    while let Some((start, end)) = attribute_span(&out, name) {
        out.replace_range(start..end, "");
    }
    out
}

fn attribute_span(tag: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = tag.as_bytes();
    let needle = name.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && tag[i..i + needle.len()].eq_ignore_ascii_case(name)
        {
            let mut end = i + needle.len();
            let mut cursor = end;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor < bytes.len() && bytes[cursor] == b'=' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len() && (bytes[cursor] == b'"' || bytes[cursor] == b'\'') {
                    let quote = bytes[cursor];
                    cursor += 1;
                    while cursor < bytes.len() && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    if cursor < bytes.len() {
                        cursor += 1;
                    }
                } else {
                    while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'>' {
                        cursor += 1;
                    }
                }
                end = cursor;
            }
            let start = if i > 0 && bytes[i - 1].is_ascii_whitespace() { i - 1 } else { i };
            return Some((start, end));
        }
        i += 1;
    }
    None
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let needle = name.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && tag[i..i + needle.len()].eq_ignore_ascii_case(name)
        {
            let mut cursor = i + needle.len();
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b'=' {
                return None;
            }
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() {
                return None;
            }
            if bytes[cursor] == b'"' || bytes[cursor] == b'\'' {
                let quote = bytes[cursor];
                cursor += 1;
                let start = cursor;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                return Some(tag[start..cursor].to_owned());
            }
            let start = cursor;
            while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'>' {
                cursor += 1;
            }
            return Some(tag[start..cursor].to_owned());
        }
        i += 1;
    }
    None
}

fn disable_task_boxes(html: &str) -> String {
    map_tags(html, |tag| {
        let lower = tag.to_ascii_lowercase();
        if lower.starts_with("input")
            && lower.contains("type=\"checkbox\"")
            && lower.contains("task-checkbox")
            && !lower.contains("disabled")
        {
            Some(format!("<{tag} disabled=\"\">"))
        } else {
            None
        }
    })
}

fn flatten_wiki_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(rel) = rest.find("<a ") {
        out.push_str(&rest[..rel]);
        let after = &rest[rel..];
        let Some(tag_end) = after.find('>') else { break };
        let is_wiki = attr_value(&after[1..tag_end], "href").is_some_and(|href| href.starts_with("wiki:"));
        let content_start = tag_end + 1;
        if is_wiki {
            if let Some(close) = after[content_start..].find("</a>") {
                out.push_str("<span class=\"wikilink\">");
                out.push_str(&after[content_start..content_start + close]);
                out.push_str("</span>");
                rest = &after[content_start + close + 4..];
                continue;
            }
        }
        {
            out.push_str(&after[..content_start]);
            rest = &after[content_start..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_percent(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
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

fn image_mime(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    Some(if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".avif") {
        "image/avif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else {
        return None;
    })
}

fn embed_images(html: &str, doc_path: &str) -> Result<String, String> {
    let dir = dirname(doc_path);
    let mut error = None;
    let out = map_tags(html, |tag| {
        if error.is_some() || !tag.to_ascii_lowercase().starts_with("img") {
            return None;
        }
        let Some(src) = attr_value(tag, "src") else { return None };
        if src.starts_with("data:") || src.starts_with("http:") || src.starts_with("https:") || src.starts_with("blob:") || src.starts_with("//") {
            return None;
        }
        let decoded = decode_percent(&src);
        let bytes = decoded.as_bytes();
        let file = if decoded.starts_with('/') || decoded.starts_with('\\') || (bytes.len() > 1 && bytes[1] == b':') {
            decoded
        } else {
            join_path(&dir, &decoded)
        };
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => return None,
        };
        if !metadata.is_file() || metadata.len() > MAX_EMBEDDED_IMAGE {
            return None;
        }
        let Some(mime) = image_mime(&file) else { return None };
        match fs::read(&file) {
            Ok(bytes) => {
                let data = format!("data:{mime};base64,{}", BASE64.encode(bytes));
                Some(format!("<{}>", replace_attr(tag, "src", &data)))
            }
            Err(e) => {
                error = Some(e.to_string());
                None
            }
        }
    });
    match error {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

fn replace_attr(tag: &str, name: &str, value: &str) -> String {
    if let Some((start, end)) = attribute_span(tag, name) {
        let mut out = tag.to_owned();
        out.replace_range(start..end, &format!(" {name}=\"{value}\""));
        out
    } else {
        format!("{tag} {name}=\"{value}\"")
    }
}

fn embed_cards_to_links(html: &str) -> String {
    const CLOSE: &str = "</span></span></span>";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<span class=\"embed embed-card\"") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(CLOSE) else {
            out.push_str(after);
            return out;
        };
        let card = &after[..end + CLOSE.len()];
        let url = attr_value(card, "data-embed-url").unwrap_or_default();
        let title = attr_value(card, "data-embed-title").unwrap_or_else(|| "Video".into());
        out.push_str(&format!("<a class=\"embed-link\" href=\"{}\">▶ {} <span>{}</span></a>", escape_html(&url), escape_html(&title), escape_html(&url)));
        rest = &after[end + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

fn strip_tags(html: &str) -> String {
    map_tags(html, |_| Some(String::new()))
}

fn table_of_contents(html: &str) -> String {
    let mut items = String::new();
    for level in 1..=3 {
        let open = format!("<h{level}");
        let close = format!("</h{level}>");
        let mut rest = html;
        while let Some(start) = rest.find(&open) {
            let after = &rest[start..];
            let Some(tag_end) = after.find('>') else { break };
            let id = attr_value(&after[1..tag_end], "id").unwrap_or_default();
            let Some(end) = after[tag_end + 1..].find(&close) else { break };
            if !id.is_empty() {
                // The heading text is already HTML-escaped by the renderer.
                let text = strip_tags(&after[tag_end + 1..tag_end + 1 + end]);
                items.push_str(&format!("<li class=\"toc-{level}\"><a href=\"#{}\"><span class=\"toc-text\">{text}</span><span class=\"toc-dots\"></span></a></li>", escape_html(&id)));
            }
            rest = &after[tag_end + 1 + end + close.len()..];
        }
    }
    if items.is_empty() {
        String::new()
    } else {
        format!("<nav class=\"toc\" aria-label=\"Contents\"><h2>Contents</h2><ol>{items}</ol></nav>")
    }
}

fn cover_html(cover: &Cover) -> String {
    let line = |class: &str, text: &str, extra: &str| {
        if text.trim().is_empty() {
            String::new()
        } else {
            format!("<p class=\"{class}\"{extra}>{}</p>", escape_html(text.trim()))
        }
    };
    let date = if cover.date.is_empty() { today_iso() } else { cover.date.clone() };
    format!(
        "<section class=\"cover\" aria-label=\"Cover\">{}{}<div class=\"cover-meta\">{}{}</div></section>",
        line("cover-title", &cover.title, " role=\"heading\" aria-level=\"1\""),
        line("cover-subtitle", &cover.subtitle, ""),
        line("cover-author", &cover.author, ""),
        line("cover-date", &date, ""),
    )
}

/// ISO date from the system clock. Avoids pulling a date crate into the export path.
fn today_iso() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 / 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> Value {
        serde_json::to_value(ExportPrefs {
            pdf: PdfPrefs {
                page_size: "A4".into(), margins: "normal".into(), header: true,
                page_numbers: true, break_before_h1: false, template: "standard".into(),
            },
            toc: false, embed_images: false, theme: Some("paper".into()),
            cover: false, cover_author: String::new(),
        }).unwrap()
    }

    #[test]
    fn strips_editor_only_attributes() {
        let html = "<p data-line-start=\"1\" data-line-end=\"2\" class=\"x\">Hi</p>";
        assert_eq!(strip_line_anchors(html), "<p class=\"x\">Hi</p>");
    }

    #[test]
    fn flattens_wiki_links_without_losing_text() {
        let html = "<p><a class=\"wikilink\" href=\"wiki:Project%20notes\" rel=\"noopener noreferrer\">the notes</a></p>";
        assert_eq!(flatten_wiki_links(html), "<p><span class=\"wikilink\">the notes</span></p>");
    }

    #[test]
    fn creates_toc_from_headings() {
        let html = "<h1 id=\"one\">One</h1><h2 id=\"two\">Two &amp; three</h2>";
        assert!(table_of_contents(html).contains("<li class=\"toc-1\"><a href=\"#one\">"));
        assert!(table_of_contents(html).contains("Two &amp; three"));
    }

    #[test]
    fn cover_defaults_to_an_iso_date() {
        let html = cover_html(&Cover { title: "Title".into(), ..Cover::default() });
        assert!(html.contains("class=\"cover-title\" role=\"heading\" aria-level=\"1\">Title</p>"));
        assert!(html.contains("class=\"cover-date\">20"));
    }

    #[test]
    fn exports_a_pdf_without_a_webview() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("hello.pdf");
        let result = export_pdf(
            "C:/notes/hello.md",
            &out.to_string_lossy(),
            "# Hello

A browser-free export.
",
            &prefs(),
            &Value::Null,
        )
        .unwrap();
        let bytes = fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(result["bytes"], bytes.len() as u64);
    }

    #[test]
    fn creates_missing_parent_folders_for_exports() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("fresh").join("nested").join("note.html");
        let out = out.to_string_lossy().into_owned();
        let result = export_html("C:/notes/hello.md", &out, "# Hello\n", &prefs(), &Value::Null).unwrap();
        assert!(std::path::Path::new(&out).is_file());
        assert_eq!(result["path"], out);
        assert!(result["bytes"].as_u64().unwrap_or(0) > 0);
    }

    #[test]
    fn builds_standalone_html_without_a_webview() {
        let html = build_html("# Hello\n\nA note with [[Other]] and `code`.\n", "C:/notes/hello.md", &prefs(), &Value::Null, false).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>hello</title>"));
        assert!(html.contains("<h1 id=\"hello\">Hello</h1>"));
        assert!(!html.contains("data-line-start"));
        assert!(html.contains("data:font/woff2;base64,"));
        assert!(!html.contains("@page"));
    }
}
