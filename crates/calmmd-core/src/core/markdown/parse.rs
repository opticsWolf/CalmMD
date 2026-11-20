//! Mordant-backed parsing: front matter, headings and a top-level source map.

use mordant::{
    ast::{Arena, KindData, Meta, NodeRef},
    footnote::footnote_parser_extension,
    meta::{meta_parser_extension, MetaParserOptions},
    parser::{self, GfmOptions, ParserExtension},
    text::BasicReader,
    Result as MordantResult,
};
use serde_json::{Map, Number, Value};

use crate::core::dto::{HeadingDto, MarkdownParseResult, SourceBlockDto};

pub(crate) fn new_parser() -> parser::Parser {
    let extensions = parser::gfm(GfmOptions::default())
        .and(meta_parser_extension(MetaParserOptions::default()))
        .and(footnote_parser_extension());
    parser::Parser::with_extensions(
        parser::Options {
            attributes: true,
            auto_heading_ids: true,
            ..parser::Options::default()
        },
        extensions,
    )
}

pub(crate) struct Parsed<'a> {
    pub arena: Arena,
    pub document: NodeRef,
    pub source: &'a str,
}

pub(crate) fn parse_tree(source: &str) -> Parsed<'_> {
    let parser = new_parser();
    let mut reader = BasicReader::new(source);
    let (arena, document) = parser.parse(&mut reader);
    Parsed { arena, document, source }
}

pub fn parse_document(source: &str) -> MordantResult<MarkdownParseResult> {
    let parsed = parse_tree(source);
    let blocks = top_level_blocks(&parsed);
    let headings = headings(&parsed);
    let front_matter = match parsed.arena[parsed.document].kind_data() {
        KindData::Document(document) => meta_to_json(document.metadata()),
        _ => Value::Object(Map::new()),
    };
    Ok(MarkdownParseResult {
        headings,
        front_matter,
        blocks,
    })
}

fn line_at(source: &str, byte: usize) -> usize {
    let byte = byte.min(source.len());
    source.as_bytes()[..byte]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

fn top_level_blocks(parsed: &Parsed<'_>) -> Vec<SourceBlockDto> {
    let Parsed { arena, document, source } = parsed;
    let children: Vec<NodeRef> = arena[*document].children(arena).collect();
    let last_line = source.as_bytes().iter().filter(|&&b| b == b'\n').count();
    children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let byte_start = arena[*node].pos()?;
            let start_line = line_at(source, byte_start);
            let end_line = children
                .get(index + 1)
                .and_then(|next| arena[*next].pos())
                .map(|byte| line_at(source, byte).saturating_sub(1))
                .unwrap_or(last_line);
            Some(SourceBlockDto {
                kind: arena[*node].kind_data().kind_name().to_owned(),
                start_line,
                end_line: end_line.max(start_line),
                byte_start,
            })
        })
        .collect()
}

fn collect_text(arena: &Arena, node: NodeRef, source: &str, out: &mut String) {
    match arena[node].kind_data() {
        KindData::Text(text) => out.push_str(text.str(source)),
        KindData::CodeSpan(code) => out.push_str(&code.str(source)),
        _ => {
            for child in arena[node].children(arena) {
                collect_text(arena, child, source, out);
            }
        }
    }
}

fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            pending_space = !slug.is_empty();
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            if pending_space {
                slug.push('-');
                pending_space = false;
            }
            for lower in ch.to_lowercase() {
                slug.push(lower);
            }
        }
    }
    if slug.is_empty() {
        "section".to_owned()
    } else {
        slug
    }
}

fn headings(parsed: &Parsed<'_>) -> Vec<HeadingDto> {
    let Parsed { arena, document, source } = parsed;
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result = Vec::new();
    let mut visit = |node: NodeRef| {
        if let KindData::Heading(heading) = arena[node].kind_data() {
            let mut text = String::new();
            for child in arena[node].children(arena) {
                collect_text(arena, child, source, &mut text);
            }
            let base = slugify(&text);
            let count = seen.entry(base.clone()).or_insert(0);
            let id = if *count == 0 {
                base
            } else {
                format!("{base}-{count}")
            };
            *count += 1;
            let byte_start = arena[node].pos().unwrap_or(0);
            result.push(HeadingDto {
                level: heading.level(),
                text,
                id,
                line: line_at(source, byte_start),
                byte_start,
            });
        }
    };
    // Headings are leaf blocks, but walking the whole tree also finds headings nested in block quotes.
    let mut queue: std::collections::VecDeque<NodeRef> =
        arena[*document].children(arena).collect();
    while let Some(node) = queue.pop_front() {
        visit(node);
        queue.extend(arena[node].children(arena));
    }
    result
}

fn meta_to_json(meta: &mordant::ast::Metadata) -> Value {
    let mut map = Map::new();
    for (key, value) in meta.iter() {
        map.insert(key.clone(), meta_value_to_json(value));
    }
    Value::Object(map)
}

fn meta_value_to_json(value: &Meta) -> Value {
    match value {
        Meta::Null => Value::Null,
        Meta::Bool(value) => Value::Bool(*value),
        Meta::Int(value) => Value::Number(Number::from(*value)),
        Meta::Float(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Meta::String(value) => Value::String(value.clone()),
        Meta::Sequence(values) => Value::Array(values.iter().map(meta_value_to_json).collect()),
        Meta::Mapping(values) => {
            let mut map = Map::new();
            for (key, value) in values.iter() {
                map.insert(key.clone(), meta_value_to_json(value));
            }
            Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str = include_str!("tests/fixtures/extension-corpus.md");

    #[test]
    fn parses_headings_with_stable_ids_and_lines() {
        let parsed = parse_document(CORPUS).unwrap();
        assert_eq!(parsed.headings.len(), 5);
        assert_eq!(parsed.headings[0].text, "Project notes");
        assert_eq!(parsed.headings[0].id, "project-notes");
        assert_eq!(parsed.headings[0].line, 6); // 0-based, matching data-line-start
        assert_eq!(parsed.headings[1].id, "section-one");
        assert_eq!(parsed.headings[2].id, "section-one-1");
        assert_eq!(parsed.headings[3].id, "section-two");
        assert_eq!(parsed.headings[4].level, 3);
    }

    #[test]
    fn parses_front_matter() {
        let parsed = parse_document(CORPUS).unwrap();
        assert_eq!(parsed.front_matter["title"], "Rust core fixture");
        assert_eq!(parsed.front_matter["tags"][0], "mordant");
        assert_eq!(parsed.front_matter["draft"], false);
    }

    #[test]
    fn source_map_covers_top_level_blocks() {
        let parsed = parse_document(CORPUS).unwrap();
        assert!(parsed.blocks.len() >= 6);
        assert!(parsed.blocks.iter().any(|b| b.kind == "Paragraph"));
        assert!(parsed.blocks.iter().any(|b| b.kind == "Heading"));
        assert!(parsed.blocks.iter().any(|b| b.kind == "Table"));
        assert!(parsed.blocks.windows(2).all(|w| w[0].start_line <= w[0].end_line));
        assert!(parsed.blocks.windows(2).all(|w| w[0].start_line < w[1].start_line));
    }
}
