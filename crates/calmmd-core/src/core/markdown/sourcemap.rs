//! AST transformer that mirrors markdown-it's `data-line-start` / `data-line-end` attributes.
//!
//! Mordant records byte positions but not line ranges. CalmMD's preview and scroll-sync code needs
//! 0-based, end-exclusive line ranges on rendered block elements, so this pass derives them from
//! each block's byte position and the position of the following sibling (or nearest ancestor's
//! following sibling).

use std::fmt::Debug;

use mordant::{
    ast::{Arena, Attributes, NodeRef, NodeType},
    parser::{parser_extension, AnyAstTransformer, AstTransformer, Context, NoParserOptions},
    text::{BasicReader, MultilineValue, Reader},
    Error as MordantError,
};

pub(crate) fn line_at(source: &str, byte: usize) -> usize {
    let byte = byte.min(source.len());
    source.as_bytes()[..byte]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

pub(crate) fn last_line(source: &str) -> usize {
    let newlines = source.as_bytes().iter().filter(|&&b| b == b'\n').count();
    if source.ends_with('\n') {
        newlines
    } else {
        newlines + 1
    }
}

fn next_line_boundary(arena: &Arena, node: NodeRef, source_len: usize) -> usize {
    let mut current = node;
    while let Some(parent) = arena[current].parent() {
        if let Some(next) = arena[current].next_sibling() {
            if let Some(pos) = arena[next].pos() {
                return pos.min(source_len);
            }
        }
        current = parent;
    }
    source_len
}

pub(crate) fn block_line_range(
    source: &str,
    arena: &Arena,
    node: NodeRef,
) -> Option<(usize, usize)> {
    if arena[node].kind_data().typ() == NodeType::Inline {
        return None;
    }
    let kind = arena[node].kind_data().kind_name();
    if matches!(
        kind,
        "Document" | "FootnoteDefinition" | "LinkReferenceDefinition" | "TableCell"
    ) {
        return None;
    }
    let byte = arena[node].pos()?;
    let start = line_at(source, byte);
    // GFM table rows are one source line. Mordant leaves a trailing row without an explicit end
    // position, so deriving it from the next sibling can shorten it to `(start, start)`.
    if kind == "TableRow" {
        let row_end = source[byte.min(source.len())..]
            .find('\n')
            .map(|offset| byte + offset + 1)
            .unwrap_or(source.len());
        return Some((start, line_at(source, row_end).max(start + 1)));
    }
    let boundary = next_line_boundary(arena, node, source.len());
    let end = if boundary >= source.len() {
        last_line(source)
    } else if kind == "List" || kind == "ListItem" {
        // markdown-it includes a list's trailing blank line in its source map.
        line_at(source, boundary)
    } else {
        line_at(source, boundary).saturating_sub(1)
    };
    Some((start, end.max(start)))
}

fn insert_line_attrs(attributes: &mut Attributes, start: usize, end: usize) {
    attributes.insert("data-line-start", MultilineValue::from_string(start.to_string()));
    attributes.insert("data-line-end", MultilineValue::from_string(end.to_string()));
}

#[derive(Debug, Default)]
struct SourceLineTransformer;

impl AstTransformer for SourceLineTransformer {
    fn transform(
        &self,
        arena: &mut Arena,
        doc_ref: NodeRef,
        reader: &mut BasicReader,
        _context: &mut Context,
    ) {
        let source = reader.source();
        let mut blocks = Vec::new();
        mordant::ast::walk(arena, doc_ref, &mut |arena: &Arena, node: NodeRef, entering: bool| {
            if entering {
                if let Some((start, end)) = block_line_range(source, arena, node) {
                    blocks.push((node, start, end));
                }
            }
            Ok::<mordant::ast::WalkStatus, MordantError>(mordant::ast::WalkStatus::Continue)
        })
        .expect("source-line AST walk cannot fail");
        for (node, start, end) in blocks {
            insert_line_attrs(arena[node].attributes_mut(), start, end);
        }
    }
}

impl From<SourceLineTransformer> for AnyAstTransformer {
    fn from(value: SourceLineTransformer) -> Self {
        AnyAstTransformer::Extension(Box::new(value))
    }
}

pub(crate) fn source_line_parser_extension() -> impl mordant::parser::ParserExtension {
    parser_extension(|p| {
        p.add_ast_transformer(|| SourceLineTransformer, NoParserOptions, 100);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::markdown::parse::parse_tree;

    #[test]
    fn annotates_headings_and_paragraphs_with_end_exclusive_lines() {
        let src = "# Title\n\nPara one\nstill one\n\n- a\n- b\n";
        let parsed = parse_tree(src);
        let mut ranges = Vec::new();
        for child in parsed.arena[parsed.document].children(&parsed.arena) {
            if let Some(range) = block_line_range(src, &parsed.arena, child) {
                ranges.push((parsed.arena[child].kind_data().kind_name().to_owned(), range));
            }
        }
        assert_eq!(ranges[0].1, (0, 1));
        assert_eq!(ranges[1].1, (2, 4));
        assert_eq!(ranges[2].1, (5, 7));
    }

    #[test]
    fn lists_include_the_trailing_blank_line() {
        let src = "- a\n- b\n\nAfter\n";
        let parsed = parse_tree(src);
        let list = parsed.arena[parsed.document]
            .first_child()
            .expect("list node");
        let (start, end) = block_line_range(src, &parsed.arena, list).unwrap();
        assert_eq!((start, end), (0, 3));
    }
}

#[cfg(test)]
mod render_tests {
    use crate::core::dto::MarkdownRenderOptions;
    use crate::core::markdown::render::render_html;

    #[test]
    fn render_adds_source_line_attributes() {
        let html = render_html("# Title\n\nPara one\nstill one\n\n- a\n- b\n", &MarkdownRenderOptions::default()).unwrap().html;
        assert!(html.contains(r#"<h1 id="title" data-line-start="0" data-line-end="1">"#), "{html}");
        assert!(html.contains(r#"<p data-line-start="2" data-line-end="4">"#), "{html}");
        assert!(html.contains(r#"<ul data-line-start="5" data-line-end="7">"#), "{html}");
    }
}
