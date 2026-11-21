//! Maps the text edited on the Preview page back to Markdown byte ranges.
use gpui_kit::component::input::InputEdit;
use gpui_kit::{
    px, FontStyle, FontWeight, HighlightStyle, Hsla, StrikethroughStyle, UnderlineStyle,
};
use markdown::{mdast::Node, to_mdast, ParseOptions};
use std::ops::Range;

#[derive(Default)]
pub(super) struct Projection {
    pub text: String,
    pub heading: Option<u8>,
    parts: Vec<Part>,
}

struct Part {
    display: Range<usize>,
    source: Range<usize>,
    style: Marks,
}

#[derive(Clone, Copy, Default)]
struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
    link: bool,
}

impl Projection {
    pub fn new(source: &str, references: &str) -> Self {
        let mut options = ParseOptions::gfm();
        options.constructs.frontmatter = true;
        options.constructs.math_flow = true;
        options.constructs.math_text = true;
        let mut this = Self::default();
        if let Ok(Node::Root(root)) = to_mdast(&format!("{source}{references}"), &options) {
            if let Some(Node::Heading(heading)) = root.children.first() {
                this.heading = Some(heading.depth);
            }
            for child in &root.children {
                this.collect(child, source.len(), Marks::default());
            }
        }
        this.keep_trailing_space(source);
        this
    }

    fn keep_trailing_space(&mut self, source: &str) {
        let start = source.trim_end_matches([' ', '\t', '\n', '\r']).len();
        if start == source.len() {
            return;
        }
        let suffix = &source[start..];
        if let Some(last) = self
            .parts
            .last_mut()
            .filter(|part| part.source.end >= start && part.display.end == self.text.len())
        {
            let existing = self
                .text
                .as_bytes()
                .iter()
                .rev()
                .zip(suffix.as_bytes().iter().rev())
                .take_while(|(a, b)| a == b)
                .count()
                .min(suffix.len());
            self.text.truncate(self.text.len() - existing);
            self.text.push_str(suffix);
            last.display.end = self.text.len();
            last.source.end = source.len();
        } else if self.is_editable() || start == 0 {
            let display_start = self.text.len();
            self.text.push_str(suffix);
            self.parts.push(Part {
                display: display_start..self.text.len(),
                source: start..source.len(),
                style: Marks::default(),
            });
        }
    }

    fn collect(&mut self, node: &Node, source_len: usize, mut style: Marks) {
        match node {
            Node::Strong(_) => style.bold = true,
            Node::Emphasis(_) => style.italic = true,
            Node::Delete(_) => style.strike = true,
            Node::Link(_) | Node::LinkReference(_) => style.link = true,
            Node::Text(text) => {
                if let Some(position) = &text.position {
                    let source = position.start.offset..position.end.offset;
                    if source.end <= source_len {
                        let start = self.text.len();
                        self.text.push_str(&text.value);
                        self.parts.push(Part {
                            display: start..self.text.len(),
                            source,
                            style,
                        });
                    }
                }
                return;
            }
            // Keep the visual text of protected inline objects. Their markup is never editable here.
            Node::InlineCode(_)
            | Node::InlineMath(_)
            | Node::Image(_)
            | Node::ImageReference(_) => {
                self.text.push_str(&match node {
                    Node::Image(image) => image.alt.clone(),
                    Node::ImageReference(image) => image.alt.clone(),
                    _ => node.to_string(),
                });
                return;
            }
            Node::Definition(_)
            | Node::FootnoteDefinition(_)
            | Node::Yaml(_)
            | Node::Code(_)
            | Node::Math(_) => return,
            Node::Break(_) => self.text.push('\n'),
            _ => {}
        }
        let paragraph = matches!(
            node,
            Node::Paragraph(_) | Node::Heading(_) | Node::TableCell(_)
        );
        if paragraph && !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text.push('\n');
        }
        if let Some(children) = node.children() {
            for child in children {
                self.collect(child, source_len, style);
            }
        }
    }

    pub fn is_editable(&self) -> bool {
        !self.parts.is_empty()
    }

    pub fn decorations(
        &self,
        link: Hsla,
    ) -> impl Iterator<Item = (Range<usize>, HighlightStyle)> + '_ {
        self.parts
            .iter()
            .map(move |part| {
                let style = part.style;
                (
                    part.display.clone(),
                    HighlightStyle {
                        font_weight: style.bold.then_some(FontWeight::BOLD),
                        font_style: style.italic.then_some(FontStyle::Italic),
                        strikethrough: style.strike.then_some(StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        }),
                        color: style.link.then_some(link),
                        underline: style.link.then_some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(link),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )
            })
            .filter(|(_, style)| *style != HighlightStyle::default())
    }

    /// Map the visible selection to source bytes, retaining delimiters inside the range.
    pub fn source_selection(&self, source: &str, range: Range<usize>) -> Option<Range<usize>> {
        if self.parts.is_empty() {
            return range.is_empty().then_some(source.len()..source.len());
        }
        if range.is_empty() {
            let part = self.part_at(range.start)?;
            let offset = self.source_offset(source, part, range.start);
            return Some(offset..offset);
        }
        let first = self
            .parts
            .iter()
            .find(|part| part.display.end > range.start)?;
        let last = self
            .parts
            .iter()
            .rev()
            .find(|part| part.display.start < range.end)?;
        if first.display.start > range.start || last.display.end < range.end {
            return None;
        }
        Some(
            self.source_offset(source, first, range.start)
                ..self.source_offset(source, last, range.end),
        )
    }

    /// Write a plain typing operation without changing existing formatting.
    pub fn replace(&self, source: &str, new_text: &str, edit: Option<InputEdit>) -> Option<String> {
        let exact = edit.and_then(|edit| {
            let old = edit.start_byte..edit.old_end_byte;
            let new = edit.start_byte..edit.new_end_byte;
            self.text.get(old.clone())?;
            new_text.get(new.clone())?;
            (self.text.get(..old.start) == new_text.get(..new.start)
                && self.text.get(old.end..) == new_text.get(new.end..))
            .then_some((old, new))
        });
        let (old, inserted) = exact.unwrap_or_else(|| changed_range(&self.text, new_text));
        if self.parts.is_empty() {
            return source.is_empty().then(|| escape(new_text));
        }
        if !self
            .parts
            .iter()
            .any(|part| part.display.start <= old.start && part.display.end >= old.end)
            && !old.is_empty()
        {
            // Editing over a protected object or across a formatted run would
            // cut its source syntax. Leave that markup intact.
            let mut covered = old.start;
            for part in self
                .parts
                .iter()
                .filter(|part| part.display.end > old.start && part.display.start < old.end)
            {
                if part.display.start > covered {
                    return None;
                }
                covered = part.display.end;
            }
            if covered < old.end {
                return None;
            }
        }
        let first = if old.is_empty() {
            self.part_at(old.start)?
        } else {
            self.parts
                .iter()
                .find(|part| part.display.end > old.start)?
        };
        let mut result = source.to_owned();
        let typed = &new_text[inserted];
        let insertion = escape(typed);
        if old.is_empty()
            && old.start == first.display.end
            && typed.starts_with(char::is_whitespace)
        {
            let outside = inline_wrappers(source, first.source.clone(), first.style).end;
            if outside != first.source.end {
                result.insert_str(outside, &insertion);
                return Some(result);
            }
        }
        for part in self.parts.iter().rev().filter(|part| {
            std::ptr::eq(*part, first)
                || (part.display.end > old.start && part.display.start < old.end)
        }) {
            let start = old.start.clamp(part.display.start, part.display.end);
            let end = old.end.clamp(part.display.start, part.display.end);
            let mut range =
                self.source_offset(source, part, start)..self.source_offset(source, part, end);
            let change = if std::ptr::eq(part, first) {
                insertion.as_str()
            } else {
                ""
            };
            if range == part.source && change.is_empty() {
                range = inline_wrappers(source, range, part.style);
            }
            result.replace_range(range, change);
        }
        Some(result)
    }

    fn part_at(&self, offset: usize) -> Option<&Part> {
        self.parts
            .iter()
            .find(|part| part.display.start <= offset && part.display.end >= offset)
    }

    fn source_offset(&self, source: &str, part: &Part, offset: usize) -> usize {
        let local = offset - part.display.start;
        let visible = &self.text[part.display.clone()];
        let raw = &source[part.source.clone()];
        if raw == visible {
            return part.source.start + local;
        }
        if local == visible.len() {
            return part.source.end;
        }
        let mut offset = 0;
        for ch in visible[..local].chars() {
            let tail = &raw[offset..];
            if tail.starts_with(ch) {
                offset += ch.len_utf8();
            } else if tail
                .strip_prefix('\\')
                .is_some_and(|tail| tail.starts_with(ch))
            {
                offset += 1 + ch.len_utf8();
            } else if let Some(entity) =
                tail.strip_prefix('&').and_then(|tail| tail.split_once(';'))
            {
                offset += entity.0.len() + 2;
            } else if let Some(index) = tail.find(ch) {
                offset += index + ch.len_utf8();
            }
        }
        part.source.start + offset
    }
}

fn inline_wrappers(source: &str, mut range: Range<usize>, marks: Marks) -> Range<usize> {
    for (marker, active) in [
        ("***", marks.bold && marks.italic),
        ("___", marks.bold && marks.italic),
        ("**", marks.bold),
        ("__", marks.bold),
        ("~~", marks.strike),
        ("*", marks.italic),
        ("_", marks.italic),
    ] {
        if active
            && source[..range.start].ends_with(marker)
            && source[range.end..].starts_with(marker)
        {
            range.start -= marker.len();
            range.end += marker.len();
        }
    }
    range
}

pub(super) fn changed_range(before: &str, after: &str) -> (Range<usize>, Range<usize>) {
    let start = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let tail = before[start..]
        .chars()
        .rev()
        .zip(after[start..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    (start..before.len() - tail, start..after.len() - tail)
}

fn escape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for (i, ch) in text.char_indices() {
        let line_start = i == 0 || text[..i].ends_with('\n');
        if matches!(
            ch,
            '\\' | '*'
                | '_'
                | '~'
                | '\x60'
                | '['
                | ']'
                | '<'
                | '>'
                | '!'
                | '&'
                | '#'
                | '|'
                | '^'
                | '$'
                | '='
        ) || (line_start && matches!(ch, '-' | '+'))
            || (matches!(ch, '.' | ')')
                && text[..i].rsplit('\n').next().is_some_and(|prefix| {
                    !prefix.is_empty() && prefix.bytes().all(|byte| byte.is_ascii_digit())
                }))
        {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::Projection;

    #[test]
    fn typing_inside_formatted_text_preserves_markdown_and_escapes_typed_markup() {
        let source = "# Hello **world** and [link](https://example.test)";
        let view = Projection::new(source, "");
        assert_eq!(view.text, "Hello world and link");
        assert_eq!(view.heading, Some(1));
        let edited = view
            .replace(source, "Hello worldly and link", None)
            .unwrap();
        assert_eq!(
            edited,
            "# Hello **worldly** and [link](https://example.test)"
        );
        let literal = Projection::new("", "")
            .replace("", "**typed**", None)
            .unwrap();
        assert_eq!(Projection::new(&literal, "").text, "**typed**");
    }

    #[test]
    fn typing_keeps_pending_spaces_and_line_breaks_visible() {
        let source = "**word**";
        let baseline = Projection::new(source, "");
        let with_space = baseline.replace(source, "word ", None).unwrap();
        assert_eq!(Projection::new(&with_space, "").text, "word ");
        let source = "first  \n\n";
        let projection = Projection::new(source, "");
        assert_eq!(projection.text, source);
        assert_eq!(
            projection
                .replace(source, "first  \n\nsecond", None)
                .as_deref(),
            Some("first  \n\nsecond")
        );
    }

    #[test]
    fn formatting_selection_maps_through_hidden_markup_and_unicode() {
        let source = "**Grüße** and \\[text\\]";
        let view = Projection::new(source, "");
        assert_eq!(view.text, "Grüße and [text]");
        assert_eq!(
            &source[view.source_selection(source, 0.."Grüße".len()).unwrap()],
            "Grüße"
        );
        assert_eq!(
            view.replace(source, "Grüße and [note]", None).unwrap(),
            "**Grüße** and \\[note\\]"
        );
    }
}
