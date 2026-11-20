//! Rust core services: Markdown parsing/rendering, syntax themes and native spellchecking.

pub mod autosave;
pub mod buffers;
pub mod dto;
pub mod export;
pub mod highlight;
pub mod history;
pub mod links;
pub mod markdown;
pub mod paths;
pub mod session;
pub mod spell;
pub mod workspace;

use dto::{HighlightCodeResult, MarkdownParseResult, MarkdownRenderOptions, MarkdownRenderResult};

pub fn markdown_parse(text: String) -> Result<MarkdownParseResult, String> {
    markdown::parse_document(&text).map_err(|error| error.to_string())
}

pub fn markdown_render(
    text: String,
    options: Option<MarkdownRenderOptions>,
) -> Result<MarkdownRenderResult, String> {
    markdown::render_html(&text, &options.unwrap_or_default()).map_err(|error| error.to_string())
}

pub fn markdown_highlight_code(
    language: String,
    code: String,
    theme: Option<String>,
) -> HighlightCodeResult {
    let theme = highlight::resolve(theme.as_deref(), None);
    markdown::highlight_code_block(&language, &code, &theme)
}
