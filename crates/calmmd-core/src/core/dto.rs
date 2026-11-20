//! Serde DTOs shared by Rust core commands and the TypeScript platform layer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadingDto {
    pub level: u8,
    pub text: String,
    pub id: String,
    pub line: usize,
    pub byte_start: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBlockDto {
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub byte_start: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownParseResult {
    pub headings: Vec<HeadingDto>,
    pub front_matter: Value,
    pub blocks: Vec<SourceBlockDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedText {
    pub text: String,
    pub eol: String,
    pub bom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRenderOptions {
    #[serde(default = "default_true")]
    pub gfm: bool,
    #[serde(default = "default_true")]
    pub footnotes: bool,
    #[serde(default = "default_true")]
    pub attributes: bool,
    #[serde(default = "default_true")]
    pub auto_heading_ids: bool,
    #[serde(default)]
    pub highlight_code: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for MarkdownRenderOptions {
    fn default() -> Self {
        Self {
            gfm: true,
            footnotes: true,
            attributes: true,
            auto_heading_ids: true,
            highlight_code: false,
            theme: default_theme(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "CalmMD Snow".to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRenderResult {
    pub html: String,
    pub source_map: Vec<SourceBlockDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightCodeResult {
    pub html: String,
}

/// One open document mirror sent from the webview to the Rust workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDocumentDto {
    pub path: String,
    pub text: String,
    #[serde(default = "default_eol")]
    pub eol: String,
    #[serde(default)]
    pub bom: bool,
    #[serde(default)]
    pub mtime_ms: Option<f64>,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub conflict: bool,
}

fn default_eol() -> String {
    "\n".into()
}

/// Full live-state snapshot that makes the Rust workspace authoritative for MCP calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshotDto {
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub documents: Vec<WorkspaceDocumentDto>,
    #[serde(default)]
    pub active_document: Option<String>,
}

/// A state change the webview must reflect in its controllers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEventDto {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_dir: Option<bool>,
}

impl WorkspaceEventDto {
    pub fn content(path: String, text: String, mtime_ms: f64) -> Self {
        Self {
            kind: "content".into(),
            path: Some(path),
            text: Some(text),
            mtime_ms: Some(mtime_ms),
            from: None,
            to: None,
            is_dir: None,
        }
    }

    pub fn renamed(from: String, to: String) -> Self {
        Self {
            kind: "renamed".into(),
            path: None,
            text: None,
            mtime_ms: None,
            from: Some(from),
            to: Some(to),
            is_dir: None,
        }
    }

    pub fn created(path: String, is_dir: bool) -> Self {
        Self {
            kind: "created".into(),
            path: Some(path),
            text: None,
            mtime_ms: None,
            from: None,
            to: None,
            is_dir: Some(is_dir),
        }
    }

    pub fn trashed(path: String, is_dir: bool) -> Self {
        Self {
            kind: "trashed".into(),
            path: Some(path),
            text: None,
            mtime_ms: None,
            from: None,
            to: None,
            is_dir: Some(is_dir),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_options_default_to_the_current_preview_behaviour() {
        let opts = MarkdownRenderOptions::default();
        assert!(opts.gfm && opts.footnotes && opts.attributes && opts.auto_heading_ids);
        assert!(!opts.highlight_code);
        assert_eq!(opts.theme, "CalmMD Snow");
    }

    #[test]
    fn render_options_accept_partial_json() {
        let opts: MarkdownRenderOptions = serde_json::from_str(r#"{"highlightCode":true,"theme":"Dracula"}"#).unwrap();
        assert!(opts.gfm);
        assert!(opts.highlight_code);
        assert_eq!(opts.theme, "Dracula");
    }
}
