//! Native Markdown preview extensions for CalmMD.
//!
//! GPUI's text view parses Markdown itself and never creates a browser DOM, so
//! DOMPurify is not part of this path. CalmMD-specific syntax is handled with
//! native plugins instead of injected HTML/JavaScript.

use crate::{platform::Platform, state::WorkspaceState};
use gpui_kit::{
    component::{
        button::Button,
        label::Label,
        text::{
            FrontmatterPlugin, MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
            TextView, markdown_ast,
        },
    },
    base::StyledExt,
    Entity, IntoElement, ParentElement, SharedString, Styled, Window, px,
};
use std::sync::Arc;

fn is_page_break(source: Option<&str>) -> bool {
    source
        .map(|line| {
            let line = line.trim();
            line.eq_ignore_ascii_case("---pagebreak---")
                || line.eq_ignore_ascii_case("\\pagebreak")
                || line.eq_ignore_ascii_case("\\newpage")
                || line.eq_ignore_ascii_case("<!-- pagebreak -->")
        })
        .unwrap_or(false)
}

fn embed_kind(url: &str) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    if lower.contains("youtu.be/") || lower.contains("youtube.com/") || lower.contains("youtube-nocookie.com/") {
        Some("youtube")
    } else if lower.contains("vimeo.com/") || lower.contains("player.vimeo.com/") {
        Some("vimeo")
    } else if [".mp4", ".webm", ".m4v", ".mov", ".ogv", ".ogg"]
        .iter()
        .any(|ext| lower.split('?').next().is_some_and(|path| path.ends_with(ext)))
    {
        Some("video")
    } else {
        None
    }
}

fn embed_page_url(url: &str) -> String {
    if embed_kind(url) == Some("video") {
        url.to_owned()
    } else {
        // Native views never load a third-party player without an explicit click.
        url.to_owned()
    }
}

const SUPERSCRIPTS: &[(&str, &str)] = &[
    ("0", "⁰"), ("1", "¹"), ("2", "²"), ("3", "³"), ("4", "⁴"),
    ("5", "⁵"), ("6", "⁶"), ("7", "⁷"), ("8", "⁸"), ("9", "⁹"),
];
const SUBSCRIPTS: &[(&str, &str)] = &[
    ("0", "₀"), ("1", "₁"), ("2", "₂"), ("3", "₃"), ("4", "₄"),
    ("5", "₅"), ("6", "₆"), ("7", "₇"), ("8", "₈"), ("9", "₉"),
];

/// A deliberately small MathML-free rendering: enough for reading simple
/// note equations natively while keeping the source selectable and portable.
fn readable_math(source: &str) -> String {
    let mut out = source
        .replace("\\alpha", "α")
        .replace("\\beta", "β")
        .replace("\\gamma", "γ")
        .replace("\\pi", "π")
        .replace("\\sum", "Σ")
        .replace("\\sqrt", "√")
        .replace("\\times", "×")
        .replace("\\rightarrow", "→");
    for (digit, glyph) in SUPERSCRIPTS {
        out = out.replace(&format!("^{digit}"), glyph);
    }
    for (digit, glyph) in SUBSCRIPTS {
        out = out.replace(&format!("_{digit}"), glyph);
    }
    out.replace("\\(", "").replace("\\)", "")
}

struct MathPlugin;

impl MarkdownPlugin for MathPlugin {
    fn name(&self) -> &str {
        "calmmd-math"
    }

    fn parse(&self, node: &markdown_ast::Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        match node {
            markdown_ast::Node::InlineMath(math) => {
                let readable = readable_math(&math.value);
                Some(
                    MarkdownNode::new(self.name(), readable.clone())
                        .text(readable)
                        .markdown(format!("${}$", math.value)),
                )
            }
            markdown_ast::Node::Math(math) => {
                let readable = readable_math(&math.value);
                Some(
                    MarkdownNode::new(self.name(), readable.clone())
                        .text(readable)
                        .markdown(format!("$$\n{}\n$$", math.value)),
                )
            }
            _ => None,
        }
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        let readable = node.data::<String>().cloned().unwrap_or_default();
        gpui_kit::div()
            .px(px(4.0))
            .rounded_md()
            .bg(gpui_kit::rgb(0xe8a23b))
            .child(Label::new(readable))
    }
}

struct MermaidPlugin;

impl MarkdownPlugin for MermaidPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "calmmd-mermaid"
    }

    fn parse(&self, node: &markdown_ast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else { return None };
        if code.lang.as_deref()?.eq_ignore_ascii_case("mermaid") {
            Some(
                MarkdownNode::new(self.name(), code.value.clone())
                    .text(code.value.clone())
                    .markdown(cx.node_source(node).unwrap_or(&code.value).to_owned()),
            )
        } else {
            None
        }
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        let source = node.data::<String>().cloned().unwrap_or_default();
        gpui_kit::div()
            .v_flex()
            .gap(px(4.0))
            .p(px(10.0))
            .rounded_md()
            .border_1()
            .border_color(gpui_kit::rgb(0x2459b3))
            .child(Label::new("Mermaid diagram · native source view"))
            .child(Label::new(source))
    }
}

struct EmbedPlugin {
    platform: Arc<dyn Platform>,
}

impl MarkdownPlugin for EmbedPlugin {
    fn name(&self) -> &str {
        "calmmd-embed"
    }

    fn parse(&self, node: &markdown_ast::Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::Image(image) = node else { return None };
        embed_kind(&image.url)?;
        let label = if image.alt.is_empty() { "Video".to_owned() } else { image.alt.clone() };
        Some(
            MarkdownNode::new(self.name(), (image.url.clone(), label.clone()))
                .text(label)
                .markdown(image.url.clone()),
        )
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        let (url, label) = node
            .data::<(String, String)>()
            .cloned()
            .unwrap_or_else(|| (String::new(), "Video".into()));
        let platform = self.platform.clone();
        Button::new(SharedString::from(format!("embed:{url}")))
            .label(format!("▶ {label} · opens externally"))
            .on_click(move |_, _, cx| platform.open_url(&embed_page_url(&url), cx))
    }
}

struct PageBreakPlugin;

impl MarkdownPlugin for PageBreakPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "calmmd-page-break"
    }

    fn parse(&self, node: &markdown_ast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        if !matches!(node, markdown_ast::Node::ThematicBreak(_)) {
            return None;
        }
        if !is_page_break(cx.node_source(node)) {
            return None;
        }
        Some(MarkdownNode::new(self.name(), ()).text("Page break"))
    }

    fn render(&self, _node: &MarkdownNode, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        gpui_kit::div()
            .h_flex()
            .items_center()
            .gap(px(6.0))
            .child(gpui_kit::div().h(px(1.0)).flex_1().bg(gpui_kit::rgb(0x8c909a)))
            .child(Label::new("page break"))
            .child(gpui_kit::div().h(px(1.0)).flex_1().bg(gpui_kit::rgb(0x8c909a)))
    }
}

struct WikiLinkPlugin {
    state: Entity<WorkspaceState>,
}

impl MarkdownPlugin for WikiLinkPlugin {
    fn name(&self) -> &str {
        "calmmd-wiki-link"
    }

    fn parse(&self, node: &markdown_ast::Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::Text(text) = node else { return None };
        let value = text.value.trim();
        let inner = value.strip_prefix("[[")?.strip_suffix("]]")?;
        if inner.is_empty() {
            return None;
        }
        Some(
            MarkdownNode::new(self.name(), inner.to_owned())
                .text(inner.to_owned())
                .markdown(value.to_owned()),
        )
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        let target = node.data::<String>().cloned().unwrap_or_default();
        let state = self.state.clone();
        Button::new(SharedString::from(format!("wiki:{target}")))
            .label(target.clone())
            .on_click(move |_, window, cx| {
                state.update(cx, |state, cx| state.open_wiki_link(&target, window, cx));
            })
    }
}

/// Builds the Markdown extension registry used by the preview.
pub fn extensions(
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
) -> MarkdownExtensions {
    let mut extensions = MarkdownExtensions::default()
        .frontmatter()
        .raw_html(false)
        .plugin(FrontmatterPlugin::new())
        .plugin(MathPlugin)
        .plugin(MermaidPlugin)
        .plugin(EmbedPlugin { platform })
        .plugin(PageBreakPlugin)
        .plugin(WikiLinkPlugin { state });

    for name in ["note", "tip", "important", "warning", "caution"] {
        extensions = extensions.block_surface(name, move |_window, _cx| {
            let color: gpui_kit::Hsla = match name {
                "tip" => gpui_kit::rgb(0x2f8a4a).into(),
                "important" => gpui_kit::rgb(0x7c4dcc).into(),
                "warning" => gpui_kit::rgb(0xb7791f).into(),
                "caution" => gpui_kit::rgb(0xc2412d).into(),
                _ => gpui_kit::rgb(0x2f6fd6).into(),
            };
            gpui_kit::div()
                .v_flex()
                .p(px(10.0))
                .rounded_md()
                .border_l_2()
                .border_color(color)
                .bg(gpui_kit::rgb(0xf5f6f8))
                .child(Label::new(name.to_uppercase()))
        });
    }
    extensions
}

/// Renders a note with native GPUI elements and no browser HTML parsing.
pub fn view(
    text: &str,
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
) -> TextView {
    TextView::markdown("calmmd-preview", text.to_owned())
        .markdown_extensions(extensions(state, platform))
        .selectable(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_calm_page_break_markers() {
        assert!(is_page_break(Some("---pagebreak---")));
        assert!(is_page_break(Some("\\newpage")));
        assert!(is_page_break(Some("<!-- pagebreak -->")));
        assert!(!is_page_break(Some("---")));
    }

    #[test]
    fn recognises_embed_urls_without_network_access() {
        assert_eq!(embed_kind("https://youtu.be/abc123"), Some("youtube"));
        assert_eq!(embed_kind("https://vimeo.com/123"), Some("vimeo"));
        assert_eq!(embed_kind("clip.mp4"), Some("video"));
        assert_eq!(embed_kind("https://example.com/note.md"), None);
    }

    #[test]
    fn renders_simple_math_readably() {
        assert_eq!(readable_math("x^2"), "x²");
        assert_eq!(readable_math("a_1 + \\alpha"), "a₁ + α");
    }
}
