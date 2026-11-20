//! CalmMD Markdown extensions for Mordant.
//!
//! The TypeScript renderer adds a handful of syntax extensions that Mordant does not know about.
//! This module ports the ones that affect the frozen extension corpus: wiki links, `==mark==`,
//! `~sub~`, `^sup^`, GitHub-style callouts, task-list checkboxes, CalmMD code blocks, math and
//! front matter. Mermaid stays in the TypeScript layer because it needs the browser-side renderer.

use std::any::TypeId;
use std::sync::Arc;

use mordant::{
    as_extension_data, as_kind_data, as_type_data,
    ast::{
        walk, Arena, CodeBlock, HtmlBlock, KindData, NodeRef, Paragraph, RawHtml, TableBody, Task,
        Text, WalkStatus,
    },
    footnote::{FootnoteDefinition, FootnoteReference},
    highlighter::{highlight_spans, HighlightingMode},
    math::{render_math, MathData},
    parser::{
        parser_extension, AnyInlineParser, Context, InlineParser, NoParserOptions, ParserExtension,
        PRIORITY_LINK,
    },
    renderer::{
        html::{self, render_attributes, Options as HtmlOptions, ParagraphRendererContext},
        BoxRenderNode, NodeRenderer, NodeRendererRegistry, NoRendererOptions, PostRender, Render,
        RenderNode, RendererOptions, TextWrite,
    },
    text::{MultilineValue, Reader},
    Error as MordantError, Result as MordantResult,
};

use super::sourcemap::line_at;
use crate::core::highlight::resolve;

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
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

/// JavaScript's `encodeURIComponent`, used for `wiki:` hrefs by the TypeScript renderer.
fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn append_class(arena: &mut Arena, node: NodeRef, source: &str, class: &str) {
    let existing = arena[node]
        .attributes()
        .get("class")
        .map(|value| value.str(source).into_owned())
        .unwrap_or_default();
    let mut classes: Vec<&str> = existing.split_whitespace().collect();
    if !classes.contains(&class) {
        classes.push(class);
    }
    arena[node].attributes_mut().insert(
        "class",
        MultilineValue::from_string(classes.join(" ")),
    );
}

fn append_class_raw(arena: &mut Arena, node: NodeRef, source: &str, class: &str) {
    let existing = arena[node]
        .attributes()
        .get("class")
        .map(|value| value.str(source).into_owned())
        .unwrap_or_default();
    arena[node].attributes_mut().insert(
        "class",
        MultilineValue::from_string(format!("{existing} {class}").trim().to_owned()),
    );
}

fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}


#[derive(Debug)]
struct CalmInlineParser;

impl InlineParser for CalmInlineParser {
    fn trigger(&self) -> &[u8] {
        b"[=~^"
    }

    fn parse(
        &self,
        arena: &mut Arena,
        _parent_ref: NodeRef,
        reader: &mut mordant::text::BlockReader,
        _ctx: &mut Context,
    ) -> Option<NodeRef> {
        let (line, _segment) = reader.peek_line_bytes()?;
        match line.first().copied()? {
            b'[' => parse_wiki_link(arena, reader, &line),
            b'=' => parse_delimited(arena, reader, &line, b"==", "mark", false),
            // GFM owns `~~strikethrough~~`; CalmMD uses a single `~` for subscript.
            b'~' if line.get(1) != Some(&b'~') => {
                parse_delimited(arena, reader, &line, b"~", "sub", true)
            }
            b'^' => parse_delimited(arena, reader, &line, b"^", "sup", true),
            _ => None,
        }
    }
}

impl From<CalmInlineParser> for AnyInlineParser {
    fn from(value: CalmInlineParser) -> Self {
        AnyInlineParser::Extension(Box::new(value))
    }
}

fn parse_wiki_link<'a>(
    arena: &mut Arena,
    reader: &mut mordant::text::BlockReader,
    line: &[u8],
) -> Option<NodeRef> {
    if line.len() < 4 || line.get(1) != Some(&b'[') {
        return None;
    }
    let end = find_bytes(line, b"]]", 2)?;
    let inner = &line[2..end];
    if inner.is_empty() || inner.iter().any(|b| matches!(b, b'[' | b']' | b'\n' | b'\r')) {
        return None;
    }
    let inner = std::str::from_utf8(inner).ok()?;
    let (target, alias) = match inner.split_once('|') {
        Some((target, alias)) => (target.trim(), alias.trim()),
        None => {
            let target = inner.trim();
            (target, target.split('#').next().unwrap_or(target).trim())
        }
    };
    if target.is_empty() {
        return None;
    }
    let label = if alias.is_empty() { target } else { alias };
    reader.advance(end + 2);
    Some(arena.new_node(RawHtml::new(MultilineValue::from_string(format!(
        "<a class=\"wikilink\" href=\"wiki:{}\" rel=\"noopener noreferrer\">{}</a>",
        encode_uri_component(target),
        escape_html(label)
    )))))
}

fn parse_delimited<'a>(
    arena: &mut Arena,
    reader: &mut mordant::text::BlockReader,
    line: &[u8],
    delimiter: &[u8],
    tag: &str,
    no_spaces: bool,
) -> Option<NodeRef> {
    let start = delimiter.len();
    let end = find_bytes(line, delimiter, start)?;
    let content = &line[start..end];
    if content.is_empty() || content.iter().any(|b| matches!(b, b'\n' | b'\r')) {
        return None;
    }
    if no_spaces && content.iter().any(u8::is_ascii_whitespace) {
        return None;
    }
    let content = std::str::from_utf8(content).ok()?;
    reader.advance(end + delimiter.len());
    Some(arena.new_node(RawHtml::new(MultilineValue::from_string(format!(
        "<{tag}>{}</{tag}>",
        escape_html(content)
    )))))
}

fn callout_kind(first_line: &str) -> Option<(&'static str, &'static str)> {
    let marker = first_line.trim_start().trim_end();
    let marker = marker.strip_prefix("[!")?.strip_suffix(']')?;
    match marker.to_ascii_lowercase().as_str() {
        "note" => Some(("note", "Note")),
        "tip" => Some(("tip", "Tip")),
        "important" => Some(("important", "Important")),
        "warning" => Some(("warning", "Warning")),
        "caution" => Some(("caution", "Caution")),
        _ => None,
    }
}

fn replace_text_node(arena: &mut Arena, parent: NodeRef, node: NodeRef, value: String) {
    let replacement = arena.new_node(Text::new(value));
    parent.replace_child(arena, node, replacement);
}

fn strip_callout_marker(arena: &mut Arena, paragraph: NodeRef, source: &str, mut bytes: usize) {
    let children: Vec<NodeRef> = arena[paragraph].children(arena).collect();
    for child in children {
        if bytes == 0 {
            break;
        }
        let text = match arena[child].kind_data() {
            KindData::Text(text) => text.str(source).to_owned(),
            _ => break,
        };
        if text.len() <= bytes {
            bytes -= text.len();
            child.remove(arena);
        } else {
            let rest = text[bytes..].to_string();
            replace_text_node(arena, paragraph, child, rest.trim_start().to_owned());
            bytes = 0;
        }
    }
    if bytes == 0 {
        if let Some(child) = arena[paragraph].first_child() {
            let trimmed = match arena[child].kind_data() {
                mordant::ast::KindData::Text(text) => Some(text.str(source).trim_start().to_owned()),
                _ => None,
            };
            if let Some(trimmed) = trimmed {
                replace_text_node(arena, paragraph, child, trimmed);
            }
        }
    }
}

#[derive(Debug)]
struct CalmAstTransformer;

impl mordant::parser::AstTransformer for CalmAstTransformer {
    fn transform(
        &self,
        arena: &mut Arena,
        doc_ref: NodeRef,
        reader: &mut mordant::text::BasicReader,
        _context: &mut Context,
    ) {
        let source = reader.source();
        let mut blockquotes = Vec::new();
        let mut task_items = Vec::new();
        mordant::ast::walk(arena, doc_ref, &mut |arena: &Arena, node: NodeRef, entering: bool| {
            if entering {
                match arena[node].kind_data() {
                    mordant::ast::KindData::Blockquote(_) => blockquotes.push(node),
                    mordant::ast::KindData::ListItem(_) => {
                        if as_kind_data!(arena, node, ListItem).task().is_some() {
                            task_items.push(node);
                        }
                    }
                    _ => {}
                }
            }
            Ok::<WalkStatus, mordant::Error>(WalkStatus::Continue)
        })
        .expect("CalmMD AST walk cannot fail");

        for blockquote in blockquotes {
            let Some(first) = arena[blockquote].first_child() else {
                continue;
            };
            let mordant::ast::KindData::Paragraph(_) = arena[first].kind_data() else {
                continue;
            };
            let first_line = as_type_data!(arena, first, Block)
                .source()
                .first()
                .map(|line| line.str(source).into_owned())
                .unwrap_or_default();
            let Some((kind, title)) = callout_kind(&first_line) else {
                continue;
            };
            append_class(arena, blockquote, source, "callout");
            append_class(arena, blockquote, source, &format!("callout-{kind}"));
            strip_callout_marker(arena, first, source, format!("[!{}]", kind.to_ascii_uppercase()).len());

            let title_paragraph = arena.new_node(Paragraph::new());
            let title_text = arena.new_node(Text::new(title));
            title_paragraph.append_child(arena, title_text);
            append_class(arena, title_paragraph, source, "callout-title");
            let first_child = arena[blockquote]
                .first_child()
                .expect("callout still has a body paragraph");
            blockquote.insert_before(arena, first_child, title_paragraph);
        }

        for item in task_items {
            append_class(arena, item, source, "task-list-item");
            if let Some(list) = arena[item].parent() {
                // The TypeScript baseline calls attrJoin once per task item, which leaves the
                // class twice in the frozen snapshot. Preserve that quirk during rollout.
                append_class_raw(arena, list, source, "contains-task-list");
            }
        }
    }
}

impl From<CalmAstTransformer> for mordant::parser::AnyAstTransformer {
    fn from(value: CalmAstTransformer) -> Self {
        mordant::parser::AnyAstTransformer::Extension(Box::new(value))
    }
}

struct CalmFootnoteReferenceRenderer<W> {
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmFootnoteReferenceRenderer<W> {
    fn new(_reg: std::rc::Rc<std::cell::RefCell<mordant::context::ContextKeyRegistry>>, html_opts: HtmlOptions) -> Self {
        Self {
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmFootnoteReferenceRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        _source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        if entering {
            let note = as_extension_data!(arena, node_ref, FootnoteReference);
            let n = note.index();
            let suffix = if note.ref_index() <= 1 {
                String::new()
            } else {
                format!(":{}", note.ref_index())
            };
            self.writer.write_html(
                w,
                &format!(
                    "<sup class=\"footnote-ref\"><a href=\"#fn{n}\" id=\"fnref{n}{suffix}\" rel=\"noopener noreferrer\">[{n}]</a></sup>"
                ),
            )?;
        }
        Ok(WalkStatus::SkipChildren)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmFootnoteReferenceRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<FootnoteReference>(), BoxRenderNode::new(self));
    }
}

/// Skips definitions during the main walk; the post-render hook emits the whole footnote section.
struct CalmFootnoteDefinitionRenderer<W> {
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmFootnoteDefinitionRenderer<W> {
    fn new(_reg: std::rc::Rc<std::cell::RefCell<mordant::context::ContextKeyRegistry>>) -> Self {
        Self { _phantom: std::marker::PhantomData }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmFootnoteDefinitionRenderer<W> {
    fn render_node<'a>(
        &self,
        _w: &mut W,
        _source: &'a str,
        _arena: &'a Arena,
        _node_ref: NodeRef,
        _entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        Ok(WalkStatus::SkipChildren)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmFootnoteDefinitionRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<FootnoteDefinition>(), BoxRenderNode::new(self));
    }
}

struct CalmFootnotePostRender<W> {
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmFootnotePostRender<W> {
    fn new(_reg: std::rc::Rc<std::cell::RefCell<mordant::context::ContextKeyRegistry>>, html_opts: HtmlOptions, _options: NoRendererOptions) -> Self {
        Self {
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> PostRender<W> for CalmFootnotePostRender<W> {
    fn post_render(
        &self,
        w: &mut W,
        source: &str,
        arena: &Arena,
        root: NodeRef,
        render: &dyn Render<W>,
        context: &mut mordant::renderer::Context,
    ) -> MordantResult<()> {
        let mut definitions = Vec::new();
        walk(arena, root, &mut |arena: &Arena, node: NodeRef, entering: bool| {
            if entering {
                if let KindData::Extension(_) = arena[node].kind_data() {
                    if arena[node].kind_data().kind_name() == "FootnoteDefinition" {
                        definitions.push(node);
                    }
                }
            }
            Ok::<WalkStatus, MordantError>(WalkStatus::Continue)
        })
        .expect("footnote AST walk cannot fail");
        definitions.retain(|node| {
            let note = as_extension_data!(arena, *node, FootnoteDefinition);
            !note.references().is_empty()
        });
        if definitions.is_empty() {
            return Ok(());
        }
        definitions.sort_by_key(|node| {
            let note = as_extension_data!(arena, *node, FootnoteDefinition);
            note.references().first().copied().unwrap_or(usize::MAX)
        });

        self.writer.write_html(w, "<hr class=\"footnotes-sep\">\n")?;
        self.writer.write_html(w, "<section class=\"footnotes\">\n<ol class=\"footnotes-list\">\n")?;
        for def in definitions {
            let note = as_extension_data!(arena, def, FootnoteDefinition);
            let n = note.index();
            self.writer
                .write_html(w, &format!("<li id=\"fn{n}\" class=\"footnote-item\">"))?;
            let children: Vec<NodeRef> = arena[def].children(arena).collect();
            let last_is_paragraph = children
                .last()
                .is_some_and(|child| matches!(arena[*child].kind_data(), KindData::Paragraph(_)));
            let body = if last_is_paragraph {
                &children[..children.len() - 1]
            } else {
                &children[..]
            };
            for child in body {
                render.render(w, source, arena, *child, context)?;
            }
            if last_is_paragraph {
                let paragraph = *children.last().expect("checked above");
                self.writer.write_safe_str(w, "<p")?;
                render_attributes(w, source, arena[paragraph].attributes(), None)?;
                self.writer.write_safe_str(w, ">")?;
                for child in arena[paragraph].children(arena) {
                    render.render(w, source, arena, child, context)?;
                }
            }
            for reference in note.references() {
                let suffix = if *reference <= 1 {
                    String::new()
                } else {
                    format!(":{reference}")
                };
                self.writer.write_html(
                    w,
                    &format!(" <a href=\"#fnref{n}{suffix}\" class=\"footnote-backref\" rel=\"noopener noreferrer\">↩︎</a>"),
                )?;
            }
            if last_is_paragraph {
                self.writer.write_safe_str(w, "</p>\n")?;
            }
            self.writer.write_html(w, "</li>\n")?;
        }
        self.writer.write_html(w, "</ol>\n</section>\n")?;
        Ok(())
    }
}

pub(crate) fn footnote_renderer_extension<'r>() -> impl mordant::renderer::html::RendererExtension<'r, String> {
    html::renderer_extension(move |renderer| {
        renderer.add_node_renderer(CalmFootnoteReferenceRenderer::new, NoRendererOptions);
        renderer.add_node_renderer(CalmFootnoteDefinitionRenderer::new, NoRendererOptions);
        renderer.add_post_render_hook(CalmFootnotePostRender::new, NoRendererOptions, 500);
    })
}

pub(crate) fn calm_parser_extension() -> impl ParserExtension {
    parser_extension(|p| {
        p.add_inline_parser(|| CalmInlineParser, NoParserOptions, PRIORITY_LINK - 100);
        p.add_ast_transformer(|| CalmAstTransformer, NoParserOptions, 50);
    })
}

#[derive(Clone)]
pub(crate) struct CodeRendererOptions {
    pub highlight_code: bool,
    pub theme: String,
}

impl RendererOptions for CodeRendererOptions {}

struct CalmCodeBlockRenderer<W> {
    options: CodeRendererOptions,
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmCodeBlockRenderer<W> {
    fn with_options(html_opts: HtmlOptions, options: CodeRendererOptions) -> Self {
        Self {
            options,
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmCodeBlockRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        if !entering {
            return Ok(WalkStatus::Continue);
        }
        let block = as_kind_data!(arena, node_ref, CodeBlock);
        let mut code = String::new();
        for line in block.value().iter(source) {
            code.push_str(&line);
        }
        let code = code.strip_suffix('\n').unwrap_or(&code);
        let lang = block.language_str(source).unwrap_or("");

        if lang == "math" || lang == "latex" {
            let markup = render_math_latex(code, true);
            self.writer.write_safe_str(w, "<div class=\"math-block\"")?;
            render_attributes(w, source, arena[node_ref].attributes(), None)?;
            self.writer.write_safe_str(w, ">")?;
            self.writer.write_html(w, &markup)?;
            self.writer.write_safe_str(w, "</div>\n")?;
            return Ok(WalkStatus::Continue);
        }

        self.writer.write_safe_str(w, "<pre class=\"code-block\"")?;
        render_attributes(w, source, arena[node_ref].attributes(), None)?;
        if !lang.is_empty() {
            self.writer.write_safe_str(w, " data-lang=\"")?;
            self.writer.write(w, lang)?;
            self.writer.write_safe_str(w, "\"")?;
        }
        self.writer.write_safe_str(w, "><code>")?;
        if self.options.highlight_code {
            let spans = highlight_spans(
                lang,
                code,
                &resolve(Some(&self.options.theme), None),
                &HighlightingMode::Attribute,
            );
            self.writer.write_html(w, &spans)?;
        } else {
            self.writer.raw_write(w, code)?;
        }
        self.writer.write_safe_str(w, "</code></pre>\n")?;
        Ok(WalkStatus::Continue)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmCodeBlockRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<CodeBlock>(), BoxRenderNode::new(self));
    }
}

/** katex-rs emits KaTeX 0.16 class names; CalmMD ships KaTeX 0.19 CSS. */
fn normalize_math_classes(markup: &str) -> String {
    markup
        .replace("class=\"base\"", "class=\"katex-base\"")
        .replace("class=\"strut\"", "class=\"katex-strut\"")
        .replace("class=\"sizing ", "class=\"katex-sizing ")
        .replace("class=\"sizing\"", "class=\"katex-sizing\"")
}

fn render_math_latex(latex: &str, display: bool) -> String {
    let markup = render_math(latex.trim(), display, "both").unwrap_or_else(|err| {
        let message = escape_html(&err);
        Arc::from(format!("<span class=\"katex-error\">{message}</span>"))
    });
    normalize_math_classes(&markup)
}

struct CalmMathRenderer<W> {
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmMathRenderer<W> {
    fn with_options(html_opts: HtmlOptions, _options: NoRendererOptions) -> Self {
        Self {
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmMathRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        _source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        if entering {
            let math = as_extension_data!(arena, node_ref, MathData);
            let markup = render_math_latex(math.latex(), math.display());
            self.writer.write_html(w, &markup)?;
        }
        Ok(WalkStatus::Continue)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmMathRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<MathData>(), BoxRenderNode::new(self));
    }
}

struct CalmHtmlBlockRenderer<W> {
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmHtmlBlockRenderer<W> {
    fn with_options(html_opts: HtmlOptions, _options: NoRendererOptions) -> Self {
        Self {
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmHtmlBlockRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        if !entering {
            return Ok(WalkStatus::Continue);
        }
        self.writer
            .write_safe_str(w, "<div class=\"html-block\"")?;
        render_attributes(w, source, arena[node_ref].attributes(), None)?;
        self.writer.write_safe_str(w, ">")?;
        let block = as_kind_data!(arena, node_ref, HtmlBlock);
        for line in block.value().iter(source) {
            self.writer.write_html(w, &line)?;
        }
        self.writer.write_safe_str(w, "</div>\n")?;
        Ok(WalkStatus::Continue)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmHtmlBlockRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<HtmlBlock>(), BoxRenderNode::new(self));
    }
}

struct CalmTableBodyRenderer<W> {
    writer: html::Writer,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: TextWrite> CalmTableBodyRenderer<W> {
    fn with_options(html_opts: HtmlOptions, _options: NoRendererOptions) -> Self {
        Self {
            writer: html::Writer::with_options(html_opts),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for CalmTableBodyRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut mordant::renderer::Context,
    ) -> MordantResult<WalkStatus> {
        if !entering {
            self.writer.write_safe_str(w, "</tbody>\n")?;
            return Ok(WalkStatus::Continue);
        }
        self.writer.write_safe_str(w, "<tbody")?;
        render_attributes(w, source, arena[node_ref].attributes(), None)?;
        self.writer.write_safe_str(w, ">\n")?;
        Ok(WalkStatus::Continue)
    }
}

impl<'r, W: TextWrite + 'r> NodeRenderer<'r, W> for CalmTableBodyRenderer<W> {
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'r, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<TableBody>(), BoxRenderNode::new(self));
    }
}

fn render_task_checkbox(
    w: &mut dyn TextWrite,
    source: &str,
    arena: &Arena,
    item_ref: NodeRef,
    _context: &mut mordant::renderer::Context,
    paragraph: &ParagraphRendererContext,
) -> MordantResult<()> {
    let line = line_at(source, arena[item_ref].pos().unwrap_or(0));
    let checked = matches!(as_kind_data!(arena, item_ref, ListItem).task(), Some(Task::Completed));
    let writer = paragraph.writer();
    writer.write_safe_str(w, "<input type=\"checkbox\" class=\"task-checkbox\" data-task-line=\"")?;
    let line = line.to_string();
    w.write_str(&line)?;
    if checked {
        writer.write_safe_str(w, "\" checked=\"\">")
    } else {
        writer.write_safe_str(w, "\">")
    }
}

pub(crate) fn calm_renderer_extension<'r>(
    options: CodeRendererOptions,
) -> impl mordant::renderer::html::RendererExtension<'r, String> {
    let task_options = html::ParagraphRendererOptions {
        render_task_list_item: Some(Box::new(render_task_checkbox)),
        ..html::ParagraphRendererOptions::default()
    };
    html::renderer_extension(move |renderer| {
        renderer.add_node_renderer(CalmCodeBlockRenderer::with_options, options.clone());
        renderer.add_node_renderer(CalmMathRenderer::with_options, NoRendererOptions);
        renderer.add_node_renderer(CalmHtmlBlockRenderer::with_options, NoRendererOptions);
        renderer.add_node_renderer(CalmTableBodyRenderer::with_options, NoRendererOptions);
        renderer.add_node_renderer(html::ParagraphRenderer::with_options, task_options);
    })
}

/// The meta parser removes front matter from the AST. The preview still renders a quiet code
/// block for it, so prepend the same markup the TypeScript `frontMatterPlugin` produces.
pub(crate) fn front_matter_prelude(source: &str) -> String {
    let mut lines = source.split('\n').map(|line| line.trim_end_matches('\r'));
    let first = match lines.next() {
        Some(first) => first.trim_end(),
        None => return String::new(),
    };
    if first != "---" && first != "+++" {
        return String::new();
    }
    let all: Vec<&str> = source
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let limit = all.len().min(400);
    let close = (1..limit).find(|&i| {
        let line = all[i].trim_end();
        line == first || (first == "---" && line == "...")
    });
    let Some(close) = close else {
        return String::new();
    };
    let body = all[1..close].join("\n");
    format!(
        "<pre class=\"front-matter\" data-line-start=\"0\" data-line-end=\"{}\"><code>{}</code></pre>\n",
        close + 1,
        escape_html(&body)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dto::MarkdownRenderOptions;
    use crate::core::markdown::render::render_html;

    fn render(source: &str) -> String {
        render_html(source, &MarkdownRenderOptions::default())
            .unwrap()
            .html
    }

    #[test]
    fn renders_wiki_links() {
        let html = render("See [[Project notes|the notes]].\n");
        assert!(html.contains(r#"<a class="wikilink" href="wiki:Project%20notes" rel="noopener noreferrer">the notes</a>"#), "{html}");
    }

    #[test]
    fn renders_mark_subscript_and_superscript() {
        let html = render("==mark== ~sub~ ^sup^\n");
        assert!(html.contains("<mark>mark</mark>"), "{html}");
        assert!(html.contains("<sub>sub</sub>"), "{html}");
        assert!(html.contains("<sup>sup</sup>"), "{html}");
    }

    #[test]
    fn renders_callouts() {
        let html = render("> [!NOTE]\n> Body.\n");
        assert!(html.contains(r#"<blockquote class="callout callout-note""#), "{html}");
        assert!(html.contains(r#"<p class="callout-title">Note</p>"#), "{html}");
        assert!(html.contains("Body."), "{html}");
        assert!(!html.contains("[!NOTE]"), "{html}");
    }

    #[test]
    fn renders_task_checkbox_metadata() {
        let html = render("- [ ] todo\n- [x] done\n");
        assert!(html.contains(r#"class="contains-task-list contains-task-list""#), "{html}");
        assert!(html.contains(r#"class="task-list-item""#), "{html}");
        assert!(html.contains(r#"data-task-line="0""#), "{html}");
        assert!(html.contains(r#"data-task-line="1""#), "{html}");
        assert!(html.contains("checked=\"\""), "{html}");
    }

    #[test]
    fn renders_math_with_the_bundled_katex_css_class_names() {
        let html = render("Inline $x^2$ and a block:\n\n$$y^2$$
");
        assert!(html.contains("class=\"katex-base\""), "{html}");
        assert!(html.contains("class=\"katex-strut\""), "{html}");
        assert!(html.contains("class=\"katex-sizing"), "{html}");
        assert!(html.contains("class=\"katex-display\""), "{html}");
    }

    #[test]
    fn renders_footnotes_with_the_preview_markup() {
        let html = render("Text[^one].\n\n[^one]: Footnote body.\n");
        assert!(html.contains(r##"<sup class="footnote-ref"><a href="#fn1" id="fnref1" rel="noopener noreferrer">[1]</a></sup>"##), "{html}");
        assert!(html.contains(r#"<hr class="footnotes-sep">"#), "{html}");
        assert!(html.contains(r#"<section class="footnotes">"#), "{html}");
        assert!(html.contains(r#"<li id="fn1" class="footnote-item">"#), "{html}");
        assert!(html.contains("class=\"footnote-backref\""), "{html}");
    }

    #[test]
    fn renders_fenced_math_blocks() {
        let html = render("```math\nx^2\n```\n");
        assert!(html.contains("class=\"math-block\""), "{html}");
        assert!(html.contains("class=\"katex-base\""), "{html}");
    }

    #[test]
    fn renders_front_matter_prelude() {
        let prelude = front_matter_prelude("---\ntitle: Test\n---\n\nBody\n");
        assert!(prelude.starts_with(r#"<pre class="front-matter" data-line-start="0" data-line-end="3">"#));
        assert!(prelude.contains("title: Test"));
    }
}
