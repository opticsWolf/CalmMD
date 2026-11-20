//! Mordant-backed HTML rendering and standalone code highlighting.

use mordant::{
    footnote::footnote_parser_extension,
    highlighter::{highlight_code as mordant_highlight_code, HighlightingMode},
    math::{
        math_html_renderer_extension, math_inline_html_renderer_extension, math_parser_extension,
        MathParserOptions, MathRendererOptions,
    },
    meta::{meta_parser_extension, MetaParserOptions},
    new_markdown_to_html_string,
    parser::{self, GfmOptions, ParserExtension},
    renderer::html::{self, RendererExtension},
    Result as MordantResult,
};

use super::extensions::{
    calm_parser_extension, calm_renderer_extension, footnote_renderer_extension, front_matter_prelude,
    CodeRendererOptions,
};
use super::parse::parse_document;
use super::sourcemap::source_line_parser_extension;
use crate::core::dto::{HighlightCodeResult, MarkdownRenderOptions, MarkdownRenderResult};
use crate::core::highlight::resolve;

fn renderer_options() -> html::Options {
    html::Options {
        // The webview still sanitises with DOMPurify, so raw HTML can pass through the renderer.
        allows_unsafe: true,
        ..html::Options::default()
    }
}

fn parser_options(opts: &MarkdownRenderOptions) -> parser::Options {
    parser::Options {
        attributes: opts.attributes,
        auto_heading_ids: opts.auto_heading_ids,
        ..parser::Options::default()
    }
}

fn render_with<'r, P, R>(
    source: &str,
    opts: &MarkdownRenderOptions,
    parser_extension: P,
    renderer_extension: R,
) -> MordantResult<String>
where
    P: parser::ParserExtension,
    R: html::RendererExtension<'r, String>,
{
    let render = new_markdown_to_html_string(
        parser_options(opts),
        renderer_options(),
        parser_extension,
        renderer_extension,
    );
    let mut html = String::with_capacity(source.len() * 2);
    render(&mut html, source)?;
    let html = format!("{}{}", front_matter_prelude(source), html);
    Ok(html)
}

pub fn render_html(source: &str, opts: &MarkdownRenderOptions) -> MordantResult<MarkdownRenderResult> {
    let meta_ext = meta_parser_extension(MetaParserOptions::default());
    let footnote_ext = footnote_parser_extension();
    let footnote_renderer = footnote_renderer_extension();

    // Phase 1 always uses GFM: it matches the current TypeScript renderer (tables, tasks,
    // strikethrough) and keeps the extension types concrete. `options.gfm` stays in the DTO for
    // the parity tests and a later opt-out.
    let make_parser_extension = || {
        parser::gfm(GfmOptions::default())
            .and(meta_ext)
            .and(footnote_ext)
            .and(source_line_parser_extension())
            .and(calm_parser_extension())
            .and(math_parser_extension(MathParserOptions::default()))
    };

    let html = if opts.highlight_code {
        render_with(
            source,
            opts,
            make_parser_extension(),
            footnote_renderer
                .and(math_html_renderer_extension(MathRendererOptions::default()))
                .and(math_inline_html_renderer_extension(
                    mordant::math::MathInlineRendererOptions::default(),
                ))
                .and(calm_renderer_extension(CodeRendererOptions {
                    highlight_code: true,
                    theme: opts.theme.clone(),
                })),
        )?
    } else {
        render_with(
            source,
            opts,
            make_parser_extension(),
            footnote_renderer
                .and(math_html_renderer_extension(MathRendererOptions::default()))
                .and(math_inline_html_renderer_extension(
                    mordant::math::MathInlineRendererOptions::default(),
                ))
                .and(calm_renderer_extension(CodeRendererOptions {
                    highlight_code: false,
                    theme: opts.theme.clone(),
                })),
        )?
    };

    Ok(MarkdownRenderResult {
        html,
        source_map: parse_document(source)?.blocks,
    })
}

pub fn highlight_code_block(
    language: &str,
    code: &str,
    theme: &str,
) -> HighlightCodeResult {
    HighlightCodeResult {
        html: mordant_highlight_code(
            language,
            code,
            &resolve(Some(theme), None),
            &HighlightingMode::Attribute,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str = include_str!("tests/fixtures/extension-corpus.md");
    const SCROLL_FIXTURE: &str = include_str!("tests/fixtures/scroll-sync-500.md");
    const TS_SNAPSHOT: &str = include_str!("../../../../../src/render/__snapshots__/markdown.test.ts.snap");
    const SCROLL_ANCHORS: &str =
        include_str!("../../../../../src/render/__snapshots__/scroll-sync-500.anchors.json");

    /// Only serializer differences: DOMPurify decodes code-block quotes, and KaTeX 0.16/0.19
    /// emit the same declaration in a different order.
    fn normalize(html: &str) -> String {
        html.replace("\r\n", "\n")
            .replace("&quot;", "\"")
            .replace("margin-right:0.05em;top:-3.063em;", "top:-3.063em;margin-right:0.05em;")
    }

    fn typescript_baseline() -> String {
        let start = TS_SNAPSHOT.find("= `\n").expect("snapshot opener") + 3;
        let end = TS_SNAPSHOT.rfind("`;\n").expect("snapshot closer");
        let value = TS_SNAPSHOT[start..end].trim();
        let value = value.strip_prefix('\"').and_then(|v| v.strip_suffix('\"')).expect("snapshot string");
        normalize(value)
    }

    #[test]
    fn matches_the_typescript_extension_corpus_snapshot() {
        let result = render_html(CORPUS, &MarkdownRenderOptions::default()).unwrap();
        assert_eq!(normalize(&result.html), typescript_baseline());
    }

    #[test]
    fn renders_the_extension_corpus() {
        let result = render_html(CORPUS, &MarkdownRenderOptions::default()).unwrap();
        assert!(result.html.contains("Project notes"));
        assert!(result.html.contains("<table"));
        assert!(result.html.contains("footnote"));
        assert_eq!(result.source_map.len(), parse_document(CORPUS).unwrap().blocks.len());
    }

    fn anchors(html: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = html[i..].find("data-line-start=\"") {
            let start = i + rel + "data-line-start=\"".len();
            let end_attr = html[start..].find('"').expect("quoted line");
            let start_line = html[start..start + end_attr].parse().unwrap();
            let after = start + end_attr + 1;
            let rest = &html[after..];
            if let Some(rel2) = rest.find("data-line-end=\"") {
                let p = after + rel2 + "data-line-end=\"".len();
                let n = html[p..].find('"').expect("quoted line");
                let end_line = html[p..p + n].parse().unwrap();
                out.push((start_line, end_line));
                i = p + n + 1;
            } else {
                break;
            }
        }
        out
    }

    #[test]
    fn source_line_anchors_match_the_typescript_snapshot() {
        let rust = render_html(CORPUS, &MarkdownRenderOptions::default()).unwrap().html;
        assert_eq!(anchors(&rust), anchors(&typescript_baseline()));
    }

    #[test]
    fn source_line_anchors_match_the_typescript_500_line_fixture() {
        assert_eq!(SCROLL_FIXTURE.lines().count(), 500);
        let baseline: Vec<(usize, usize)> = serde_json::from_str(SCROLL_ANCHORS).unwrap();
        let rust = render_html(SCROLL_FIXTURE, &MarkdownRenderOptions::default()).unwrap().html;
        assert_eq!(anchors(&rust), baseline);
    }

    #[test]
    fn highlighting_emits_inline_styles() {
        let result = highlight_code_block("rust", "fn main() {}", "CalmMD Snow");
        assert!(result.html.contains("<pre"));
        assert!(result.html.contains("#7a3e9d"), "keyword colour missing: {}", result.html);
    }

    #[test]
    fn code_highlighting_can_be_enabled_during_render() {
        let opts = MarkdownRenderOptions {
            highlight_code: true,
            ..MarkdownRenderOptions::default()
        };
        let result = render_html(CORPUS, &opts).unwrap();
        assert!(result.html.contains("#2f5e9e"), "function colour missing: {}", result.html);
    }
}

