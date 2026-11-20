//! Native Takumi PDF backend.
//!
//! The webview still builds the self-contained export HTML (fonts, images and styles are inlined);
//! this module parses that HTML with Takumi and writes a paged vector PDF. Headings stay selectable,
//! links become PDF annotations and the running bands use Takumi's page-number hooks.

use std::{collections::HashSet, path::Path, sync::Arc};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Deserializer};
use takumi_core::{
    html::{FromHtmlOptions, parse_html},
    layout::node::{Node, NodeKind},
    resources::font::{FontSource, Fonts},
    style::{Lang, StyleSheet},
};
use takumi_pdf::{
    Band, PageMargin, PageMargins, PageOptions, PageOverride, PageRules, PdfError, PdfMetadata,
    PdfOptions, UncoveredText, render,
};

/// Options produced by `src/export/takumi.ts`. They mirror Takumi's browser API so the WASM
/// fallback and the native command accept the same payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TakumiPdfOptions {
    pub size: String,
    pub margin: TakumiMargins,
    /// HTML for the repeating header, or `false`/`null` when disabled.
    #[serde(default, deserialize_with = "false_or_html")]
    pub header: Option<String>,
    /// HTML for the repeating footer, or `false`/`null` when disabled.
    #[serde(default, deserialize_with = "false_or_html")]
    pub footer: Option<String>,
    #[serde(default)]
    pub pages: TakumiPageRules,
    #[serde(default)]
    pub outline: bool,
    pub metadata: TakumiMetadata,
    pub lang: String,
    /// Number level-two headings explicitly for the academic template. Takumi's counter scope
    /// keeps the TOC heading's value where Chromium resets it at the next h1.
    #[serde(default, rename = "headingNumbers")]
    pub heading_numbers: bool,
    /// Numeric CSS font-weight applied to inherited body text. Explicit heading weights win.
    #[serde(default = "default_font_weight", rename = "fontWeight")]
    pub font_weight: f32,
}

fn default_font_weight() -> f32 {
    350.0
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TakumiMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TakumiPageRules {
    #[serde(default)]
    pub first: TakumiPageOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TakumiPageOverride {
    /// `false` disables the band on that page; `null` inherits the document band.
    #[serde(default)]
    pub header: Option<bool>,
    #[serde(default)]
    pub footer: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TakumiMetadata {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
}

/// Accepts Takumi's browser `false` band value as well as `null` and HTML strings.
fn false_or_html<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BandValue {
        Disabled(bool),
        Html(String),
        Null,
    }

    match BandValue::deserialize(deserializer)? {
        BandValue::Null | BandValue::Disabled(false) => Ok(None),
        BandValue::Disabled(true) => Err(serde::de::Error::custom("true is not a PDF band")),
        BandValue::Html(html) if html.is_empty() => Ok(None),
        BandValue::Html(html) => Ok(Some(html)),
    }
}

/// Reads a CSS `url(...)` value, tolerating optional quotes.
fn css_url(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\''))) {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Extracts every embedded font URL from a stylesheet. Exports carry their fonts as `data:` URIs.
fn embedded_font_urls(css: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = css;

    while let Some(start) = rest.find("url(") {
        rest = &rest[start + 4..];
        let Some(end) = rest.find(')') else { break };
        let url = css_url(&rest[..end]);
        if url.starts_with("data:font/") || url.starts_with("data:application/octet-stream") {
            urls.push(url.to_owned());
        }
        rest = &rest[end + 1..];
    }

    urls
}

fn decode_font_url(url: &str) -> Result<Vec<u8>, String> {
    let (header, body) = url.split_once(',').ok_or("Malformed embedded font URL.")?;
    if !header.contains("base64") {
        return Err("Embedded fonts must use base64 data URLs.".into());
    }
    BASE64.decode(body).map_err(|e| format!("Could not decode an embedded font: {e}"))
}

/// Takumi does not implement CSS `target-counter()`, but it resolves the same value through a
/// `targetPageNumber` child of the linking element. The TS export keeps `target-counter()` for
/// Paged.js, so adapt only the native renderer's copy of the HTML.
fn adapt_html(html: &str) -> String {
    let html = html.replace(
        "<span class=\"toc-dots\"></span></a>",
        "<span class=\"toc-dots\"></span><span class=\"targetPageNumber toc-page-number\"></span></a>",
    );
    // Takumi does not draw native form controls. Keep the checkbox mark as a plain span so
    // task state survives the PDF export.
    // Takumi's absolute-positioned inline elements do not consistently reserve the list item's
    // padding, so lay the replacement out in flow: pull it into the existing left gutter and add a
    // small right gap. The SVG background avoids relying on a font glyph for the tick.
    let checkbox = "position:static;display:inline-block;box-sizing:border-box;width:1.02em;height:1.02em;margin:0 .55em 0 -1.7em;vertical-align:-.12em;border:1px solid #9aa0aa;border-radius:2px;background-color:#fff;background-repeat:no-repeat;background-position:center;background-size:9px 9px";
    let check = "data:image/svg+xml,%3Csvg%20xmlns=%27http://www.w3.org/2000/svg%27%20viewBox=%270%200%2012%2012%27%3E%3Cpath%20d=%27M2%206.5L4.8%209.2L10%203.5%27%20fill=%27none%27%20stroke=%27%234d7fbf%27%20stroke-width=%271.8%27%20stroke-linecap=%27round%27%20stroke-linejoin=%27round%27/%3E%3C/svg%3E";
    let checked = format!(
        "<span class=\"task-checkbox\" style=\"{checkbox};background-image:url({check})\"></span>"
    );
    let html = html
        .replace(
            "<input type=\"checkbox\" class=\"task-checkbox\" checked=\"\" disabled=\"\">",
            &checked,
        )
        .replace(
            "<input type=\"checkbox\" class=\"task-checkbox\" disabled=\"\">",
            &format!("<span class=\"task-checkbox\" style=\"{checkbox}\"></span>"),
        );
    // Mermaid and inline SVG diagrams can name system fonts that are not embedded in the export.
    // Map the common browser defaults to CalmMD's bundled Literata face.
    let html = html
        .replace("font-family%3D%22Georgia%22", "font-family%3D%22Literata%20Variable%22")
        .replace("font-family='Georgia'", "font-family='Literata Variable'")
        .replace("font-family=\"Georgia\"", "font-family=\"Literata Variable\"");
    // The footnote return arrow is not covered by the bundled Latin fonts. A plain `back` label
    // is more useful than a placeholder glyph in a PDF.
    let html = html.replace("↩︎</a>", "back</a>");
    html
}

/// Chromium resets the academic h2 counter at every h1; Takumi keeps the TOC heading's value.
/// Number real h2 nodes in document order instead of trying to rewrite the HTML string.
fn number_headings(node: &mut Node, number: &mut usize) {
    if node.tag_name().is_some_and(|tag| tag.eq_ignore_ascii_case("h1")) {
        *number = 0;
    }
    if node.tag_name().is_some_and(|tag| tag.eq_ignore_ascii_case("h2")) {
        *number += 1;
        let prefix = format!("{number}. ");
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            match &mut current.kind {
                NodeKind::Container { children } => {
                    for child in children.iter_mut().rev() {
                        pending.push(child);
                    }
                }
                NodeKind::Text(text) if !text.text.trim().is_empty() => {
                    text.text.insert_str(0, &prefix);
                    break;
                }
                _ => {}
            }
        }
        return;
    }
    if let NodeKind::Container { children } = &mut node.kind {
        for child in children {
            number_headings(child, number);
        }
    }
}

fn page_size(name: &str) -> Result<PageOptions, String> {
    let page = match name {
        "a4" => PageOptions::A4,
        "letter" => PageOptions::LETTER,
        "a5" => PageOptions::A5,
        "legal" => PageOptions::LEGAL,
        other => return Err(format!("Unsupported Takumi page size: {other}")),
    };
    Ok(page)
}

fn margins(value: &TakumiMargins) -> PageMargins {
    PageMargins {
        top: PageMargin::Px(value.top),
        right: PageMargin::Px(value.right),
        bottom: PageMargin::Px(value.bottom),
        left: PageMargin::Px(value.left),
    }
}

fn band(html: Option<&str>) -> Result<Option<takumi_core::layout::node::Node>, String> {
    html.map(|markup| {
        parse_html(markup, FromHtmlOptions::default())
            .map(|parsed| parsed.node)
            .map_err(|e| format!("Could not parse a PDF running band: {e}"))
    })
    .transpose()
}

fn off(value: Option<bool>) -> Option<Band> {
    match value {
        Some(false) => Some(Band::Off),
        _ => None,
    }
}

/// Renders self-contained export HTML to PDF bytes.
pub fn render_pdf(html_content: &str, options: &TakumiPdfOptions) -> Result<Vec<u8>, String> {
    let mut html = adapt_html(html_content);
    let weight = options.font_weight.clamp(1.0, 1000.0);
    html = html.replace(
        "</head>",
        &format!("<style>.md-body {{ font-weight: {weight}; }}</style></head>"),
    );
    if options.heading_numbers {
        html = html.replace(
            "</head>",
            "<style>.md-body h2::before { content: none !important; }</style></head>",
        );
    }
    let mut parsed = parse_html(&html, FromHtmlOptions::default())
        .map_err(|e| format!("Could not parse export HTML: {e}"))?;
    if options.heading_numbers {
        let mut number = 0;
        number_headings(&mut parsed.node, &mut number);
    }

    let mut fonts = Fonts::default();
    let mut seen = HashSet::new();
    for sheet in &parsed.css {
        for url in embedded_font_urls(sheet) {
            if !seen.insert(url.clone()) {
                continue;
            }
            let bytes = decode_font_url(&url)?;
            fonts
                .load_font_file(url, FontSource::from(bytes))
                .map_err(|e| format!("Could not load an embedded font: {e}"))?;
        }
    }

    let stylesheet = Arc::new(StyleSheet::parse_owned_list_loosy(parsed.css));
    let header = band(options.header.as_deref())?;
    let footer = band(options.footer.as_deref())?;
    let pages = PageRules {
        first: PageOverride {
            header: off(options.pages.first.header),
            footer: off(options.pages.first.footer),
        },
        ..PageRules::default()
    };
    let metadata = PdfMetadata {
        title: options.metadata.title.clone(),
        creator: options.metadata.creator.clone(),
        ..PdfMetadata::default()
    };
    let lang = Lang::parse(&options.lang).map_err(|e| format!("Invalid document language: {e}"))?;
    let page = page_size(&options.size)?.with_margin(0.0);
    let page = PageOptions {
        margin: margins(&options.margin),
        ..page
    };

    let builder = PdfOptions::builder()
        .node(parsed.node)
        .stylesheet(stylesheet)
        .fonts(&fonts)
        .page(page)
        .pages(pages)
        .outline(options.outline)
        .metadata(metadata)
        .lang(Some(lang))
        .uncovered_text(UncoveredText::Placeholder);

    let pdf = match (header, footer) {
        (Some(header), Some(footer)) => render(builder.header(header).footer(footer).build()),
        (Some(header), None) => render(builder.header(header).build()),
        (None, Some(footer)) => render(builder.footer(footer).build()),
        (None, None) => render(builder.build()),
    };
    pdf.map_err(|e: PdfError| format!("Takumi PDF render failed: {e}"))
}

/// Renders HTML to a PDF file. Both shells call this directly; the browser build keeps the WASM
/// fallback.
pub fn render_pdf_file(html: &str, out: &str, options: &TakumiPdfOptions) -> Result<(), String> {
    let pdf = render_pdf(html, options)?;
    crate::fsops::write_binary(Path::new(out), &pdf).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use takumi_core::units::{ONE_MM_IN_PX, ONE_PT_IN_PX};

    fn options() -> TakumiPdfOptions {
        serde_json::from_value(json!({
            "size": "a4",
            "margin": { "top": 83, "right": 76, "bottom": 91, "left": 76 },
            "header": null,
            "footer": null,
            "pages": { "first": { "header": false, "footer": false } },
            "outline": true,
            "metadata": { "title": "Notes", "creator": "CalmMD" },
            "lang": "en",
            "headingNumbers": false
        }))
        .expect("valid options")
    }

    #[test]
    fn reads_embedded_font_urls() {
        let css = "@font-face { src: url(data:font/woff2;base64,d09GMg==) format('woff2'); }";
        assert_eq!(
            embedded_font_urls(css),
            vec!["data:font/woff2;base64,d09GMg==".to_owned()]
        );
    }

    #[test]
    fn adapts_toc_page_counters() {
        let html = r##"<nav class="toc"><ol><li><a href="#intro"><span class="toc-text">Intro</span><span class="toc-dots"></span></a></li></ol></nav>"##;
        assert!(adapt_html(html).contains("targetPageNumber"));
    }

    #[test]
    fn maps_page_sizes_and_margins() {
        let opts = options();
        let size = page_size(&opts.size).unwrap();
        assert!((size.width - 210.0 * ONE_MM_IN_PX).abs() < 0.001);
        let margin = margins(&opts.margin);
        match margin.top {
            PageMargin::Px(value) => assert_eq!(value, 83.0),
            _ => panic!("expected pixel margins"),
        }
        assert!(ONE_PT_IN_PX > 1.0);
    }

    #[test]
    fn renders_a_simple_pdf() {
        let html = r#"<!doctype html><html><head><style>
            body { font-family: sans-serif; font-size: 12px; }
        </style></head><body><div style="width:100px;height:40px;border:1px solid #333"></div></body></html>"#;
        let pdf = render_pdf(html, &options()).expect("render simple PDF");
        assert!(pdf.starts_with(b"%PDF-"), "not a PDF: {:?}", &pdf[..pdf.len().min(16)]);
    }

    /// Manual visual check: renders `$CALMMD_TAKUMI_HTML` with `$CALMMD_TAKUMI_OPTIONS`
    /// to `$CALMMD_TAKUMI_OUT` through the native backend.
    #[test]
    #[ignore]
    fn renders_file_from_env() {
        let (Ok(html_path), Ok(options_path), Ok(out)) = (
            std::env::var("CALMMD_TAKUMI_HTML"),
            std::env::var("CALMMD_TAKUMI_OPTIONS"),
            std::env::var("CALMMD_TAKUMI_OUT"),
        ) else {
            return;
        };
        let html = std::fs::read_to_string(html_path).expect("read export HTML");
        let options: TakumiPdfOptions =
            serde_json::from_str(&std::fs::read_to_string(options_path).expect("read options"))
                .expect("parse Takumi options");
        let pdf = render_pdf(&html, &options).expect("render PDF");
        std::fs::write(out, pdf).expect("write PDF");
    }
}
