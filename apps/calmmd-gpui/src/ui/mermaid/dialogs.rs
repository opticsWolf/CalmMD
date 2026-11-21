use super::*;
use crate::diagrams::{self, DiagramType, Graph};
use gpui_kit::{
    assets::IconName,
    base::Disableable as _,
    component::{
        input::EditorState,
        select::{Select, SelectState},
        IndexPath,
    },
};
use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
};

pub(super) struct Target {
    pub view: Entity<AppView>,
    pub input: Entity<EditorState>,
    pub original: SharedString,
    pub range: Range<usize>,
    pub document: Option<String>,
}

impl Target {
    fn apply(&self, replacement: &str, window: &mut Window, cx: &mut App) -> Result<(), String> {
        if self.view.read(cx).state.read(cx).active() != self.document.as_deref()
            || self.input.read(cx).value() != self.original
        {
            return Err("The document changed. Reopen this dialog to try again.".into());
        }
        if replacement != &self.original[self.range.clone()] {
            self.input.update(cx, |editor, cx| {
                editor.set_selected_range(self.range.clone(), cx);
                editor.replace(replacement.to_owned(), window, cx);
                let cursor = self.range.start + replacement.trim_end_matches('\n').len();
                editor.set_selected_range(cursor..cursor, cx);
            });
            sync_edit(&self.view, &self.input, window, cx);
        }
        Ok(())
    }
}

fn footer(cx: &App, apply: impl Fn(&mut Window, &mut App) -> bool + 'static) -> impl IntoElement {
    div()
        .h_flex()
        .justify_end()
        .gap_2()
        .child(
            Button::new("mermaid-cancel")
                .label(tr(cx, "Cancel"))
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
        .child(
            Button::new("mermaid-apply")
                .label(tr(cx, "Apply"))
                .primary()
                .on_click(move |_, window, cx| {
                    if apply(window, cx) {
                        window.close_dialog(cx);
                    }
                }),
        )
}

fn message_result(
    error: &RefCell<Option<String>>,
    result: Result<(), String>,
    window: &mut Window,
) -> bool {
    if let Err(message) = result {
        *error.borrow_mut() = Some(message);
        window.refresh();
        false
    } else {
        true
    }
}

struct NodeFields {
    id: Entity<InputState>,
    label: Entity<InputState>,
}

impl NodeFields {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            id: cx.new(|cx| InputState::new(window, cx).placeholder(tr(cx, "Node ID"))),
            label: cx.new(|cx| InputState::new(window, cx).placeholder(tr(cx, "Label (optional)"))),
        }
    }
}

pub(super) fn add_nodes(target: Target, window: &mut Window, cx: &mut App) {
    let body = target.original[target.range.clone()].to_owned();
    let Some(graph) = Graph::parse(&body) else {
        return;
    };
    let target = Rc::new(target);
    let rows = Rc::new(RefCell::new(vec![NodeFields::new(window, cx)]));
    let error = Rc::new(RefCell::new(None::<String>));
    window.open_dialog(cx, move |dialog, _, cx| {
        let mut content = div()
            .id("mermaid-node-rows")
            .v_flex()
            .gap_2()
            .max_h(px(340.))
            .overflow_y_scroll();
        let count = rows.borrow().len();
        for (index, fields) in rows.borrow().iter().enumerate() {
            let remove = rows.clone();
            let add = rows.clone();
            content = content.child(
                div()
                    .id(("mermaid-node", index))
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().w(px(106.)).child(Input::new(&fields.id)))
                    .child(div().flex_1().child(Input::new(&fields.label)))
                    .child(
                        Button::new("remove-node-row")
                            .icon(IconName::Minus)
                            .ghost()
                            .compact()
                            .accessibility_label(tr(cx, "Remove Row"))
                            .tooltip(tr(cx, "Remove Row"))
                            .disabled(count == 1)
                            .on_click(move |_, window, _| {
                                if remove.borrow().len() > 1 {
                                    remove.borrow_mut().remove(index);
                                }
                                window.refresh();
                            }),
                    )
                    .when(index + 1 == count, |row| {
                        row.child(
                            Button::new("add-node-row")
                                .icon(IconName::Plus)
                                .ghost()
                                .compact()
                                .accessibility_label(tr(cx, "Add Row"))
                                .tooltip(tr(cx, "Add Row"))
                                .on_click(move |_, window, cx| {
                                    let fields = NodeFields::new(window, cx);
                                    fields.id.read(cx).focus_handle(cx).focus(window, cx);
                                    add.borrow_mut().push(fields);
                                    window.refresh();
                                }),
                        )
                    }),
            );
        }
        if let Some(message) = error.borrow().as_ref() {
            content = content.child(Label::new(crate::locale::message(cx, message)));
        }
        let (target, rows, error, body) =
            (target.clone(), rows.clone(), error.clone(), body.clone());
        let existing: HashSet<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
        let kind = graph.kind;
        let apply = move |window: &mut Window, cx: &mut App| {
            let result = (|| {
                let mut ids = existing.clone();
                let mut declarations = String::new();
                for fields in &*rows.borrow() {
                    let id = fields.id.read(cx).value();
                    let label = fields.label.read(cx).value();
                    let (id, label) = (id.trim(), label.trim());
                    if id.is_empty() && label.is_empty() {
                        continue;
                    }
                    if !diagrams::valid_id(id) {
                        return Err("Use letters, digits, or underscores in Mermaid IDs.".into());
                    }
                    if !ids.insert(id.to_owned()) {
                        return Err("Node IDs must be unique.".into());
                    }
                    let label = diagrams::encode_label(label);
                    let line = match kind {
                        DiagramType::Flowchart if label.is_empty() => format!("  {id}\n"),
                        DiagramType::Flowchart => format!("  {id}[\"{label}\"]\n"),
                        DiagramType::Sequence if label.is_empty() => {
                            format!("  participant {id}\n")
                        }
                        DiagramType::Sequence => format!("  participant {id} as {label}\n"),
                    };
                    declarations.push_str(&line);
                }
                if declarations.is_empty() {
                    return Err("Enter at least one node ID.".into());
                }
                target.apply(&diagrams::append(&body, &declarations), window, cx)
            })();
            message_result(&error, result, window)
        };
        dialog
            .title(tr(
                cx,
                if kind == DiagramType::Sequence {
                    "Add Participants"
                } else {
                    "Add Nodes"
                },
            ))
            .child(content)
            .footer(footer(cx, apply.clone()))
            .on_ok(move |_, window, cx| apply(window, cx))
    });
}

type NodeSelect = Entity<SelectState<Vec<String>>>;

struct Connections {
    source: RefCell<String>,
    from: NodeSelect,
    to: NodeSelect,
    label: Entity<InputState>,
    selected: Cell<Option<usize>>,
    ids: Vec<String>,
    error: RefCell<Option<String>>,
}

impl Connections {
    fn new(source: String, graph: &Graph, window: &mut Window, cx: &mut App) -> Self {
        let names = graph
            .nodes
            .iter()
            .map(|node| {
                if node.id == node.label {
                    node.id.clone()
                } else {
                    format!("{} ({})", node.label, node.id)
                }
            })
            .collect::<Vec<_>>();
        let from = cx.new(|cx| SelectState::new(names.clone(), None, window, cx).searchable(true));
        let to = cx.new(|cx| SelectState::new(names, None, window, cx).searchable(true));
        let label = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(cx, "Connection label (optional)"))
        });
        Self {
            source: RefCell::new(source),
            from,
            to,
            label,
            selected: Cell::new(None),
            ids: graph.nodes.iter().map(|node| node.id.clone()).collect(),
            error: RefCell::new(None),
        }
    }

    fn select(&self, index: usize, edge: &diagrams::Edge, window: &mut Window, cx: &mut App) {
        self.selected.set(Some(index));
        for (state, id) in [(&self.from, &edge.from), (&self.to, &edge.to)] {
            let position = self.ids.iter().position(|node| node == id);
            state.update(cx, |state, cx| {
                state.set_selected_index(
                    position.map(|index| IndexPath::default().row(index)),
                    window,
                    cx,
                )
            });
        }
        self.label.update(cx, |input, cx| {
            input.set_value(edge.label.as_str(), window, cx)
        });
        window.refresh();
    }

    fn edit(&self, operation: Operation, window: &mut Window, cx: &mut App) {
        let result = (|| {
            let source = self.source.borrow();
            let graph = Graph::parse(&source)
                .ok_or("The current diagram cannot be edited in this dialog.")?;
            if operation == Operation::Disconnect {
                return graph
                    .disconnect(&source, self.selected.get().ok_or("Select a connection.")?);
            }
            let selected = |selector: &NodeSelect| {
                selector
                    .read(cx)
                    .selected_index(cx)
                    .and_then(|index| self.ids.get(index.row))
                    .map(String::as_str)
            };
            let from = selected(&self.from).ok_or("Select two existing nodes.")?;
            let to = selected(&self.to).ok_or("Select two existing nodes.")?;
            if operation == Operation::Reconnect {
                graph.reconnect(
                    &source,
                    self.selected.get().ok_or("Select a connection.")?,
                    from,
                    to,
                    Some(self.label.read(cx).value().trim()),
                )
            } else {
                graph.connect(&source, from, to, self.label.read(cx).value().trim())
            }
        })();
        match result {
            Ok(value) => {
                *self.source.borrow_mut() = value;
                self.selected.set(None);
                *self.error.borrow_mut() = None;
            }
            Err(error) => *self.error.borrow_mut() = Some(error.to_owned()),
        }
        window.refresh();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Connect,
    Disconnect,
    Reconnect,
}

pub(super) fn connections(target: Target, window: &mut Window, cx: &mut App) {
    let source = target.original[target.range.clone()].to_owned();
    let Some(graph) = Graph::parse(&source) else {
        return;
    };
    let choice = Rc::new(Connections::new(source, &graph, window, cx));
    let target = Rc::new(target);
    window.open_dialog(cx, move |dialog, _, cx| {
        let mut edges = div()
            .id("mermaid-connections")
            .v_flex()
            .gap_1()
            .max_h(px(190.))
            .overflow_y_scroll();
        if let Some(graph) = Graph::parse(&choice.source.borrow()) {
            for (index, edge) in graph.edges.into_iter().enumerate() {
                let choice = choice.clone();
                let label = if edge.label.is_empty() {
                    format!("{} \u{2192} {}", edge.from, edge.to)
                } else {
                    format!("{} \u{2192} {} \u{00b7} {}", edge.from, edge.to, edge.label)
                };
                edges = edges.child(
                    Button::new(("mermaid-edge", index))
                        .label(label)
                        .compact()
                        .when(choice.selected.get() == Some(index), |button| {
                            button.primary()
                        })
                        .on_click(move |_, window, cx| choice.select(index, &edge, window, cx)),
                );
            }
        }
        let mut controls = div().h_flex().flex_wrap().gap_1();
        for (label, operation) in [
            ("Connect", Operation::Connect),
            ("Disconnect", Operation::Disconnect),
            ("Reconnect", Operation::Reconnect),
        ] {
            let choice = choice.clone();
            controls = controls.child(
                Button::new(label)
                    .label(tr(cx, label))
                    .compact()
                    .disabled(operation != Operation::Connect && choice.selected.get().is_none())
                    .on_click(move |_, window, cx| choice.edit(operation, window, cx)),
            );
        }
        let mut content = div()
            .v_flex()
            .gap_2()
            .child(Label::new(tr(cx, "Existing Connections")))
            .child(edges)
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(Select::new(&choice.from).placeholder(tr(cx, "From"))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Select::new(&choice.to).placeholder(tr(cx, "To"))),
                    ),
            )
            .child(Input::new(&choice.label))
            .child(controls);
        if let Some(error) = choice.error.borrow().as_ref() {
            content = content.child(Label::new(crate::locale::message(cx, error)));
        }
        let (choice, target) = (choice.clone(), target.clone());
        let apply = move |window: &mut Window, cx: &mut App| {
            let result = target.apply(&choice.source.borrow(), window, cx);
            message_result(&choice.error, result, window)
        };
        dialog
            .title(tr(cx, "Edit Connections"))
            .child(content)
            .footer(footer(cx, apply.clone()))
            .on_ok(move |_, window, cx| apply(window, cx))
    });
}
