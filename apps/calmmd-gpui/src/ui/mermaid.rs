//! The contextual Mermaid tools edit the same Markdown buffer as typing.
mod dialogs;

use super::*;
use gpui_kit::{assets::IconName, component::input::EditorState};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Action)]
#[action(namespace = calmmd, no_json)]
enum DiagramAction {
    AddNodes,
    Connections,
    Subgraph,
    Comment,
    Direction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiagramKind {
    Flowchart,
    Sequence,
    Other,
}

struct Diagram {
    body: Range<usize>,
    kind: DiagramKind,
}

fn input(view: &AppView, cx: &App) -> Option<Entity<EditorState>> {
    view.state.read(cx).active()?;
    match view.view_mode {
        ViewMode::Source | ViewMode::Split => Some(view.editor.read(cx).editor.input.clone()),
        ViewMode::Live => view.document_view.read(cx).input(cx),
        ViewMode::Preview => None,
    }
}

pub(super) fn render(view: &AppView, cx: &App) -> Option<AnyElement> {
    let input = input(view, cx)?;
    let editor = input.read(cx);
    let diagram = diagram_at(&editor.value(), editor.cursor())?;
    let tools: &[(DiagramAction, &str, IconName)] = match diagram.kind {
        DiagramKind::Flowchart => &[
            (DiagramAction::AddNodes, "Add Nodes", IconName::SquarePlus),
            (
                DiagramAction::Connections,
                "Edit Connections",
                IconName::ArrowRight,
            ),
            (DiagramAction::Subgraph, "Add Subgraph", IconName::Group),
            (
                DiagramAction::Direction,
                "Diagram Direction",
                IconName::Move,
            ),
        ],
        DiagramKind::Sequence => &[
            (
                DiagramAction::AddNodes,
                "Add Participants",
                IconName::UserPlus,
            ),
            (
                DiagramAction::Connections,
                "Edit Connections",
                IconName::ArrowRight,
            ),
        ],
        DiagramKind::Other => &[],
    };
    let theme = Theme::global(cx);
    let mut row = div()
        .id("mermaid-tools")
        .h_flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .flex_shrink_0()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .overflow_x_scroll()
        .child(Label::new("Mermaid"));
    for &(action, caption, icon) in tools.iter().chain(&[(
        DiagramAction::Comment,
        "Add Comment",
        IconName::MessageSquarePlus,
    )]) {
        row = row.child(
            Button::new(caption)
                .icon(icon)
                .label(tr(cx, caption))
                .ghost()
                .compact()
                .on_click(move |_, window, cx| window.dispatch_action(Box::new(action), cx)),
        );
    }
    Some(row.into_any_element())
}

pub(super) fn register(cx: &mut App, windows: &WindowList) {
    let windows = windows.clone();
    cx.on_action(move |action: &DiagramAction, cx| {
        let Some(entry) = active_entry(&windows, cx) else {
            return;
        };
        let action = *action;
        after_window_action(cx, entry.handle, move |window, cx| {
            command(entry.view, action, window, cx)
        });
    });
}

fn command(view: Entity<AppView>, action: DiagramAction, window: &mut Window, cx: &mut App) {
    let Some(input) = input(view.read(cx), cx) else {
        return;
    };
    let editor = input.read(cx);
    let original = editor.value();
    let Some(diagram) = diagram_at(&original, editor.cursor()) else {
        return;
    };
    let state = view.read(cx).state.clone();
    let document = state.read(cx).active().map(str::to_owned);
    if matches!(action, DiagramAction::AddNodes | DiagramAction::Connections) {
        let target = dialogs::Target {
            view,
            input,
            original,
            range: diagram.body,
            document,
        };
        if action == DiagramAction::AddNodes {
            dialogs::add_nodes(target, window, cx);
        } else {
            dialogs::connections(target, window, cx);
        }
        return;
    }
    let (title, hint, initial) = match action {
        DiagramAction::Subgraph => ("Add Subgraph", "ID or ID: label", ""),
        DiagramAction::Comment => ("Add Comment", "Comment text", ""),
        DiagramAction::Direction => ("Diagram Direction", "Direction (TD, TB, BT, LR, RL)", "TD"),
        DiagramAction::AddNodes | DiagramAction::Connections => unreachable!(),
    };
    crate::menu_dialogs::text_prompt(
        window,
        cx,
        title,
        hint,
        initial,
        move |value, window, cx| {
            if state.read(cx).active() != document.as_deref() || input.read(cx).value() != original
            {
                return Err("The document changed. Reopen this dialog to try again.".into());
            }
            let (range, text, cursor) = edit_diagram(&original, &diagram, action, value)?;
            input.update(cx, |editor, cx| {
                editor.set_selected_range(range, cx);
                editor.replace(text, window, cx);
                editor.set_selected_range(cursor..cursor, cx);
            });
            sync_edit(&view, &input, window, cx);
            Ok(())
        },
    );
}

fn diagram_at(source: &str, cursor: usize) -> Option<Diagram> {
    let mut opening: Option<(char, usize, usize, usize, bool)> = None;
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next().unwrap_or_default();
        let width = if indent <= 3 && matches!(marker, '\x60' | '~') {
            trimmed.bytes().take_while(|&b| b == marker as u8).count()
        } else {
            0
        };
        if let Some((fence, fence_width, block_start, body_start, mermaid)) = opening {
            if marker == fence && width >= fence_width && trimmed[width..].trim().is_empty() {
                if mermaid
                    && (block_start..=start + line.trim_end_matches('\n').len()).contains(&cursor)
                {
                    return Some(Diagram {
                        body: body_start..start,
                        kind: kind(&source[body_start..start]),
                    });
                }
                opening = None;
            }
        } else if width >= 3 {
            let info = trimmed[width..].trim();
            if marker != '\x60' || !info.contains('\x60') {
                let mermaid = info
                    .split_whitespace()
                    .next()
                    .is_some_and(|info| info.eq_ignore_ascii_case("mermaid"));
                opening = Some((marker, width, start, start + line.len(), mermaid));
            }
        }
        start += line.len();
    }
    let (_, _, block_start, body_start, mermaid) = opening?;
    (mermaid && body_start <= source.len() && (block_start..=source.len()).contains(&cursor)).then(
        || Diagram {
            body: body_start..source.len(),
            kind: kind(&source[body_start..]),
        },
    )
}

fn kind(body: &str) -> DiagramKind {
    let header = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"));
    match header.and_then(|line| line.split_whitespace().next()) {
        Some("flowchart" | "graph") => DiagramKind::Flowchart,
        Some("sequenceDiagram") => DiagramKind::Sequence,
        _ => DiagramKind::Other,
    }
}

fn edit_diagram(
    source: &str,
    diagram: &Diagram,
    action: DiagramAction,
    value: &str,
) -> Result<(Range<usize>, String, usize), String> {
    let body = &source[diagram.body.clone()];
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("Enter a value on one line.".into());
    }
    if action == DiagramAction::Direction {
        let direction = value.to_ascii_uppercase();
        if !["TD", "TB", "BT", "LR", "RL"].contains(&direction.as_str()) {
            return Err("Choose TD, TB, BT, LR, or RL.".into());
        }
        let mut start = diagram.body.start;
        for line in body.split_inclusive('\n') {
            let header = line.trim_start();
            if let Some(name @ ("graph" | "flowchart")) = header.split_whitespace().next() {
                start += line.len() - header.len() + name.len();
                let tail = &source[start..start + header.len() - name.len()];
                let token = tail
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(';');
                let end = if ["TD", "TB", "BT", "LR", "RL"].contains(&token) {
                    start + tail.find(token).unwrap_or_default() + token.len()
                } else {
                    start
                };
                return Ok((
                    start..end,
                    format!(" {direction}"),
                    start + 1 + direction.len(),
                ));
            }
            start += line.len();
        }
        return Err("This diagram has no flowchart header.".into());
    }

    let (before_label, label) = value
        .split_once(':')
        .map_or((value, ""), |(ids, name)| (ids.trim(), name.trim()));
    let id = before_label;
    if action == DiagramAction::Subgraph && !crate::diagrams::valid_id(id) {
        return Err("Use letters, digits, or underscores in Mermaid IDs.".into());
    }
    let append = diagram.body.end;
    let separator = if append > 0 && !source[..append].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let (insert, caret) = match action {
        DiagramAction::Subgraph => {
            let label = crate::diagrams::encode_label(if label.is_empty() { id } else { label });
            let start = format!("  subgraph {id}[\"{label}\"]\n    ");
            let caret = start.len();
            (format!("{start}\n  end\n"), Some(caret))
        }
        DiagramAction::Comment => (format!("  %% {value}\n"), None),
        DiagramAction::Direction | DiagramAction::AddNodes | DiagramAction::Connections => {
            unreachable!()
        }
    };
    let cursor = append + separator.len() + caret.unwrap_or(insert.len());
    Ok((append..append, format!("{separator}{insert}"), cursor))
}

#[cfg(test)]
mod tests {
    use super::{diagram_at, edit_diagram, DiagramAction};

    #[test]
    fn tools_only_activate_in_mermaid_fences_and_keep_closing_fence() {
        let source = "outside\n~~~text\nignored\n~~~\n~~~mermaid\nflowchart LR\n  A --> B\n~~~\n";
        assert!(diagram_at(source, 2).is_none());
        assert!(diagram_at(source, source.find("ignored").unwrap()).is_none());
        let diagram = diagram_at(source, source.find("A --> B").unwrap()).unwrap();
        let (range, edit, _) =
            edit_diagram(source, &diagram, DiagramAction::Direction, "TB").unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &edit);
        assert!(edited.contains("flowchart TB\n  A --> B"));
        let (range, edit, _) =
            edit_diagram(source, &diagram, DiagramAction::Comment, "Check the flow").unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &edit);
        assert!(edited.ends_with("  %% Check the flow\n~~~\n"));
    }
}
