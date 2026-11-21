//! A source-preserving model for the Mermaid constructs used by the visual tools.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramType {
    Flowchart,
    Sequence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Rounded,
    Diamond,
    Circle,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub arrow: String,
    pub strong: bool,
    pub dashed: bool,
    pub points_to_target: bool,
    pub removal: Range<usize>,
}

#[derive(Debug)]
pub struct Group {
    pub label: String,
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct Graph {
    pub kind: DiagramType,
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub groups: Vec<Group>,
}

impl Graph {
    pub fn parse(source: &str) -> Option<Self> {
        let mut statements = statements(source).into_iter();
        let (header, _) = statements.next()?;
        let mut words = header.split_whitespace();
        let kind = match words.next()? {
            "graph" | "flowchart" => DiagramType::Flowchart,
            "sequenceDiagram" => DiagramType::Sequence,
            _ => return None,
        };
        let direction = match words.next() {
            Some("LR") => Direction::Right,
            Some("RL") => Direction::Left,
            Some("BT") => Direction::Up,
            _ => Direction::Down,
        };
        let mut graph = Self {
            kind,
            direction,
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
        };
        let mut groups: Vec<usize> = Vec::new();
        for (line, range) in statements {
            match kind {
                DiagramType::Flowchart => {
                    if let Some(subgraph) = line.strip_prefix("subgraph ") {
                        let name = endpoint(subgraph)
                            .and_then(|node| node.label)
                            .unwrap_or(subgraph);
                        groups.push(graph.groups.len());
                        graph.groups.push(Group {
                            label: display_label(name),
                            members: Vec::new(),
                        });
                    } else if line == "end" {
                        groups.pop();
                    } else {
                        let nodes = graph.flowchart_statement(line, range);
                        for &group in &groups {
                            let members = &mut graph.groups[group].members;
                            for node in &nodes {
                                if !members.contains(node) {
                                    members.push(node.clone());
                                }
                            }
                        }
                    }
                }
                DiagramType::Sequence => graph.sequence_statement(line, range),
            }
        }
        Some(graph)
    }

    fn node(&mut self, id: &str, label: Option<&str>, shape: Shape) {
        if let Some(existing) = self.nodes.iter_mut().find(|node| node.id == id) {
            if let Some(label) = label {
                existing.label = display_label(label);
                existing.shape = shape;
            }
        } else {
            self.nodes.push(Node {
                id: id.into(),
                label: display_label(label.unwrap_or(id)),
                shape,
            });
        }
    }

    fn flowchart_statement(&mut self, source: &str, range: Range<usize>) -> Vec<String> {
        let Some(mut from) = endpoint(source) else {
            return Vec::new();
        };
        if [
            "end",
            "subgraph",
            "direction",
            "class",
            "classDef",
            "linkStyle",
            "style",
            "click",
        ]
        .contains(&from.id)
        {
            return Vec::new();
        }
        let mut nodes = vec![from.clone()];
        let mut edges = Vec::new();
        let mut cursor = from.end;
        while cursor < source.len() {
            let tail = source[cursor..].trim_start();
            if tail.is_empty() {
                break;
            }
            let arrow_start = source.len() - tail.len();
            let Some((arrow, mut label, mut after)) = flow_arrow(tail) else {
                return Vec::new();
            };
            if let Some(rest) = after.trim_start().strip_prefix('|') {
                let Some((caption, next)) = rest.split_once('|') else {
                    return Vec::new();
                };
                label = caption;
                after = next;
            }
            let Some(to) = endpoint(after) else {
                return Vec::new();
            };
            let target_start = source.len() - after.len() + to.start;
            edges.push(Edge {
                from: from.id.into(),
                to: to.id.into(),
                label: display_label(label),
                arrow: arrow.into(),
                strong: arrow.contains('='),
                dashed: arrow.contains('.'),
                points_to_target: arrow.ends_with('>'),
                removal: range.start + arrow_start..range.start + target_start,
            });
            cursor = source.len() - after.len() + to.end;
            nodes.push(to.clone());
            from = to;
        }
        for node in &nodes {
            self.node(node.id, node.label, node.shape);
        }
        self.edges.extend(edges);
        nodes.into_iter().map(|node| node.id.to_owned()).collect()
    }

    fn sequence_statement(&mut self, source: &str, range: Range<usize>) {
        if let Some(tail) = source
            .strip_prefix("participant ")
            .or_else(|| source.strip_prefix("actor "))
        {
            let (id, label) = tail
                .split_once(" as ")
                .map_or((tail.trim(), None), |(id, label)| (id.trim(), Some(label)));
            if valid_id(id) {
                self.node(id, label, Shape::Rectangle);
            }
            return;
        }
        let Some((link, caption)) = source.split_once(':') else {
            return;
        };
        let Some(from) = endpoint(link) else { return };
        let tail = link[from.end..].trim_start();
        let Some(arrow) = ["-->>", "->>", "-->", "->", "--x", "-x", "--)", "-)"]
            .into_iter()
            .find(|arrow| tail.starts_with(*arrow))
        else {
            return;
        };
        let target = tail[arrow.len()..].trim().trim_start_matches(['+', '-']);
        if !valid_id(target) {
            return;
        }
        self.node(from.id, None, Shape::Rectangle);
        self.node(target, None, Shape::Rectangle);
        self.edges.push(Edge {
            from: from.id.into(),
            to: target.into(),
            label: display_label(caption),
            arrow: arrow.into(),
            strong: false,
            dashed: arrow.starts_with("--"),
            points_to_target: true,
            removal: range,
        });
    }

    pub fn has(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    pub fn connect(
        &self,
        source: &str,
        from: &str,
        to: &str,
        label: &str,
    ) -> Result<String, &'static str> {
        if !self.has(from) || !self.has(to) {
            return Err("Select two existing nodes.");
        }
        let arrow = if self.kind == DiagramType::Sequence {
            "->>"
        } else {
            "-->"
        };
        Ok(append(source, &self.connection(from, arrow, to, label)))
    }

    pub fn disconnect(&self, source: &str, index: usize) -> Result<String, &'static str> {
        let edge = self.edges.get(index).ok_or("Select a connection.")?;
        let mut replacement = source.to_owned();
        let separator = if self.kind == DiagramType::Flowchart {
            "\n  "
        } else {
            ""
        };
        replacement.replace_range(edge.removal.clone(), separator);
        if self.kind == DiagramType::Sequence {
            self.preserve_participants(&mut replacement);
        }
        Ok(replacement)
    }

    fn preserve_participants(&self, source: &mut String) {
        let current = Self::parse(source);
        let header = statements(source).first().map_or(0, |(_, range)| range.end);
        for node in self.nodes.iter().rev() {
            if current.as_ref().is_none_or(|graph| !graph.has(&node.id)) {
                source.insert_str(header, &format!("\n  participant {}", node.id));
            }
        }
    }

    pub fn reconnect(
        &self,
        source: &str,
        index: usize,
        from: &str,
        to: &str,
        label: Option<&str>,
    ) -> Result<String, &'static str> {
        if !self.has(from) || !self.has(to) {
            return Err("Select two existing nodes.");
        }
        let edge = self.edges.get(index).ok_or("Select a connection.")?;
        let label = label.unwrap_or(&edge.label);
        if edge.from == from && edge.to == to && edge.label == label {
            return Ok(source.to_owned());
        }
        let connection = self.connection(from, &edge.arrow, to, label);
        if self.kind == DiagramType::Sequence {
            let mut replacement = source.to_owned();
            replacement.replace_range(edge.removal.clone(), connection.trim());
            self.preserve_participants(&mut replacement);
            Ok(replacement)
        } else {
            let source = self.disconnect(source, index)?;
            Ok(append(&source, &connection))
        }
    }

    fn connection(&self, from: &str, arrow: &str, to: &str, label: &str) -> String {
        let label = encode_label(label);
        if self.kind == DiagramType::Sequence {
            format!(
                "  {from}{arrow}{to}: {}\n",
                if label.is_empty() { "Message" } else { &label }
            )
        } else {
            let label = if label.is_empty() {
                String::new()
            } else {
                format!("|{label}|")
            };
            format!("  {from} {arrow}{label} {to}\n")
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.eq_ignore_ascii_case("end")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub fn append(source: &str, text: &str) -> String {
    let gap = if source.is_empty() || source.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{source}{gap}{text}")
}

pub fn encode_label(label: &str) -> String {
    label
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(';', "&#59;")
        .replace(['\r', '\n'], " ")
}

fn display_label(label: &str) -> String {
    label
        .trim()
        .trim_matches('"')
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#124;", "|")
        .replace("&#59;", ";")
        .replace("&amp;", "&")
}

fn flow_arrow(tail: &str) -> Option<(&str, &str, &str)> {
    let arrow = [
        "<-->", "<-.->", "-.->", "==>", "-->", "---", "-.-", "===", "--o", "--x",
    ]
    .into_iter()
    .find(|arrow| tail.starts_with(*arrow));
    if let Some(arrow) = arrow {
        return Some((arrow, "", &tail[arrow.len()..]));
    }
    for (start, end, arrow) in [
        ("-- ", " -->", "-->"),
        ("-. ", " .->", "-.->"),
        ("== ", " ==>", "==>"),
    ] {
        if let Some((caption, remaining)) = tail
            .strip_prefix(start)
            .and_then(|tail| tail.split_once(end))
        {
            return Some((arrow, caption.trim(), remaining));
        }
    }
    None
}

#[derive(Clone)]
struct Endpoint<'a> {
    id: &'a str,
    label: Option<&'a str>,
    shape: Shape,
    start: usize,
    end: usize,
}

fn endpoint(input: &str) -> Option<Endpoint<'_>> {
    let name = input.trim_start();
    let start = input.len() - name.len();
    let length = name
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    let id = &name[..length];
    if !valid_id(id) {
        return None;
    }
    let rest = &name[length..];
    for (open, close, shape) in [
        ("((", "))", Shape::Circle),
        ("([", "])", Shape::Rounded),
        ("[[", "]]", Shape::Rectangle),
        ("{{", "}}", Shape::Diamond),
        ("[", "]", Shape::Rectangle),
        ("(", ")", Shape::Rounded),
        ("{", "}", Shape::Diamond),
    ] {
        if let Some(body) = rest.strip_prefix(open) {
            let mut quoted = false;
            for (offset, ch) in body.char_indices() {
                if ch == '"' {
                    quoted = !quoted;
                }
                if !quoted && body[offset..].starts_with(close) {
                    return Some(Endpoint {
                        id,
                        label: Some(&body[..offset]),
                        shape,
                        start,
                        end: start + length + open.len() + offset + close.len(),
                    });
                }
            }
            return None;
        }
    }
    Some(Endpoint {
        id,
        label: None,
        shape: Shape::Rectangle,
        start,
        end: start + length,
    })
}

fn statements(source: &str) -> Vec<(&str, Range<usize>)> {
    let mut output = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut edge_label = false;
    for (i, ch) in source
        .char_indices()
        .chain(std::iter::once((source.len(), '\n')))
    {
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if ch == '|' && !quoted && depth == 0 {
            edge_label = !edge_label;
        }
        let entity = ch == ';'
            && source[start..i].rsplit_once('&').is_some_and(|(_, name)| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte == b'#' || byte.is_ascii_alphanumeric())
            });
        if i == source.len()
            || ch == '\n'
            || (ch == ';' && !quoted && !edge_label && depth == 0 && !entity)
        {
            let line = &source[start..i];
            let text = line.trim();
            if !text.is_empty() && !text.starts_with("%%") {
                let text_start = start + line.len() - line.trim_start().len();
                output.push((text, text_start..text_start + text.len()));
            }
            start = i + 1;
            depth = 0;
            quoted = false;
            edge_label = false;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{DiagramType, Graph};

    #[test]
    fn grouped_nodes_and_escaped_edge_labels_are_parsed() {
        let graph =
            Graph::parse("flowchart TD\n subgraph S[\"Group\"]\n A -->|yes &#59; later| B\n end\n")
                .unwrap();
        assert_eq!(graph.groups[0].label, "Group");
        assert_eq!(graph.groups[0].members, ["A", "B"]);
        assert_eq!(graph.edges[0].label, "yes ; later");
    }

    #[test]
    fn reconnect_preserves_node_definitions_and_other_edges_in_chain() {
        let source = "flowchart LR\n A[Alpha] -->|review| B{Beta} --> C\n";
        let graph = Graph::parse(source).unwrap();
        assert_eq!(graph.edges.len(), 2);
        let edited = graph.reconnect(source, 0, "C", "A", None).unwrap();
        let result = Graph::parse(&edited).unwrap();
        assert_eq!(result.edges.len(), 2);
        assert!(result
            .edges
            .iter()
            .any(|edge| edge.from == "B" && edge.to == "C"));
        assert!(result
            .edges
            .iter()
            .any(|edge| edge.from == "C" && edge.to == "A" && edge.label == "review"));
        assert!(result
            .nodes
            .iter()
            .any(|node| node.id == "A" && node.label == "Alpha"));
        assert!(result
            .nodes
            .iter()
            .any(|node| node.id == "B" && node.label == "Beta"));
    }

    #[test]
    fn reconnecting_sequence_message_preserves_message_order() {
        let source = "sequenceDiagram\n A->>B: first\n B-->>C: second\n";
        let graph = Graph::parse(source).unwrap();
        let edited = graph.reconnect(source, 0, "A", "C", None).unwrap();
        let after = Graph::parse(&edited).unwrap();
        assert_eq!(after.edges[0].to, "C");
        assert_eq!(after.edges[0].label, "first");
        assert_eq!(after.edges[1].label, "second");
    }

    #[test]
    fn disconnect_in_sequence_keeps_implicit_participants() {
        let source = "sequenceDiagram\n A-->>B: Done\n";
        let graph = Graph::parse(source).unwrap();
        let edited = graph.disconnect(source, 0).unwrap();
        let result = Graph::parse(&edited).unwrap();
        assert_eq!(result.kind, DiagramType::Sequence);
        assert!(result.has("A") && result.has("B"));
        assert!(result.edges.is_empty());
    }
}
