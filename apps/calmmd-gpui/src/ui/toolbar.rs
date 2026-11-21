use super::*;
use gpui_kit::{assets::IconName, component::checkbox::Checkbox};
use std::{cell::RefCell, rc::Rc};

const KEY: &str = "gpui/toolbar";
const SEPARATOR: &str = "|";
const DEFAULT: &[&str] = &[
    "undo",
    "redo",
    "|",
    "heading",
    "|",
    "bold",
    "italic",
    "strike",
    "highlight",
    "code",
    "|",
    "bullets",
    "numbers",
    "task",
    "quote",
    "|",
    "link",
    "image",
    "table",
    "code-block",
    "math",
    "mermaid",
    "rule",
    "|",
    "find",
];

struct Tool {
    id: &'static str,
    label: &'static str,
    icon: IconName,
    action: fn() -> Box<dyn Action>,
}

macro_rules! tool {
    ($id:literal, $label:literal, $icon:ident, $action:path) => {
        Tool {
            id: $id,
            label: $label,
            icon: IconName::$icon,
            action: || Box::new($action),
        }
    };
}

const TOOLS: &[Tool] = &[
    tool!("undo", "Undo", Undo2, UndoDocument),
    tool!("redo", "Redo", Redo2, RedoDocument),
    tool!("heading", "Heading", Heading, ChooseHeading),
    tool!("bold", "Bold", Bold, FormatBold),
    tool!("italic", "Italic", Italic, FormatItalic),
    tool!("strike", "Strikethrough", Strikethrough, FormatStrike),
    tool!("highlight", "Highlight", Highlighter, ToggleHighlight),
    tool!("code", "Inline Code", Code, FormatCode),
    tool!("bullets", "Bullet List", List, FormatList),
    tool!("numbers", "Ordered List", ListOrdered, OrderedList),
    tool!("task", "Task List", ListTodo, FormatTask),
    tool!("quote", "Quote", TextQuote, FormatQuote),
    tool!("link", "Link", Link, FormatLink),
    tool!("image", "Image File", Image, ImageFile),
    tool!("table", "Table", Table, InsertTable),
    tool!("code-block", "Code Block", SquareCode, InsertCodeBlock),
    tool!("math", "Math", Sigma, InsertMath),
    tool!("mermaid", "Mermaid Diagram", Workflow, InsertMermaid),
    tool!("rule", "Horizontal Rule", Minus, InsertRule),
    tool!("find", "Find", Search, EditorFind),
    tool!("save", "Save", Save, SaveDocument),
    tool!("copy", "Copy Markdown", Copy, CopyMarkdown),
];

fn default_items() -> Vec<String> {
    DEFAULT.iter().map(|id| (*id).to_owned()).collect()
}

fn find(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.id == id)
}

pub(super) fn load(state: &WorkspaceState) -> Vec<String> {
    let Some(raw) = calmmd_core::kv::read(state.context(), KEY).ok().flatten() else {
        return default_items();
    };
    let Ok(mut items) = serde_json::from_str::<Vec<String>>(&raw) else {
        return default_items();
    };
    items.retain(|id| id == SEPARATOR || find(id).is_some());
    items.truncate(40);
    items
}

pub(super) fn render(items: &[String], border: Hsla, cx: &App) -> impl IntoElement {
    let mut row = div()
        .id("format-toolbar")
        .h_flex()
        .items_center()
        .h(px(38.))
        .flex_shrink_0()
        .border_b_1()
        .border_color(border)
        .px(px(8.))
        .gap(px(3.))
        .overflow_x_scroll();
    for id in items {
        if id == SEPARATOR {
            row = row.child(
                div()
                    .w(px(1.))
                    .h(px(18.))
                    .mx(px(5.))
                    .flex_shrink_0()
                    .bg(border),
            );
        } else if let Some(tool) = find(id) {
            let action = tool.action;
            row = row.child(
                Button::new(tool.id)
                    .icon(tool.icon)
                    .accessibility_label(tr(cx, tool.label))
                    .tooltip(tr(cx, tool.label))
                    .ghost()
                    .compact()
                    .flex_shrink_0()
                    .on_click(move |_, window, cx| window.dispatch_action(action(), cx)),
            );
        }
    }
    row.child(
        Button::new("customize-toolbar")
            .icon(IconName::Ellipsis)
            .accessibility_label(tr(cx, "Customize Toolbar"))
            .tooltip(tr(cx, "Customize Toolbar"))
            .ghost()
            .compact()
            .flex_shrink_0()
            .on_click(|_, window, cx| window.dispatch_action(Box::new(CustomizeToolbar), cx)),
    )
}

pub(super) fn customize(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let items = Rc::new(RefCell::new(view.read(cx).toolbar_items.clone()));
    window.open_dialog(cx, move |dialog, _, cx| {
        let mut ordered = div()
            .id("toolbar-order")
            .v_flex()
            .gap_1()
            .max_h(px(260.))
            .overflow_y_scroll();
        for (index, id) in items.borrow().iter().enumerate() {
            let title = find(id).map_or("Separator", |tool| tool.label);
            let up = items.clone();
            let down = items.clone();
            let remove = items.clone();
            ordered = ordered.child(
                div()
                    .h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().child(Label::new(tr(cx, title))))
                    .child(
                        Button::new(("toolbar-up", index))
                            .label(tr(cx, "Up"))
                            .compact()
                            .on_click(move |_, window, _| {
                                if index > 0 {
                                    up.borrow_mut().swap(index, index - 1);
                                    window.refresh();
                                }
                            }),
                    )
                    .child(
                        Button::new(("toolbar-down", index))
                            .label(tr(cx, "Down"))
                            .compact()
                            .on_click(move |_, window, _| {
                                let mut order = down.borrow_mut();
                                if index + 1 < order.len() {
                                    order.swap(index, index + 1);
                                    window.refresh();
                                }
                            }),
                    )
                    .child(
                        Button::new(("toolbar-remove", index))
                            .label(tr(cx, "Remove"))
                            .compact()
                            .on_click(move |_, window, _| {
                                remove.borrow_mut().remove(index);
                                window.refresh();
                            }),
                    ),
            );
        }
        let mut available = div().h_flex().gap_2().flex_wrap();
        for tool in TOOLS {
            let items = items.clone();
            let checked = items.borrow().iter().any(|id| id == tool.id);
            available = available.child(
                Checkbox::new(tool.id)
                    .label(tr(cx, tool.label))
                    .checked(checked)
                    .on_change(move |checked, window, _| {
                        let mut items = items.borrow_mut();
                        if *checked && items.len() < 40 {
                            items.push(tool.id.to_owned());
                        } else if !checked {
                            items.retain(|id| id != tool.id);
                        }
                        window.refresh();
                    }),
            );
        }
        let reset = items.clone();
        let separator = items.clone();
        let save = items.clone();
        let view = view.clone();
        dialog
            .title(tr(cx, "Customize Toolbar"))
            .child(div().v_flex().gap_2().child(ordered).child(available))
            .footer(
                div()
                    .h_flex()
                    .gap_2()
                    .justify_end()
                    .flex_wrap()
                    .child(
                        Button::new("toolbar-separator")
                            .label(tr(cx, "Add Separator"))
                            .on_click(move |_, window, _| {
                                if separator.borrow().len() < 40 {
                                    separator.borrow_mut().push(SEPARATOR.to_owned());
                                    window.refresh();
                                }
                            }),
                    )
                    .child(
                        Button::new("toolbar-reset")
                            .label(tr(cx, "Reset"))
                            .on_click(move |_, window, _| {
                                *reset.borrow_mut() = default_items();
                                window.refresh();
                            }),
                    )
                    .child(
                        Button::new("toolbar-cancel")
                            .label(tr(cx, "Cancel"))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("toolbar-apply")
                            .label(tr(cx, "Apply"))
                            .primary()
                            .on_click(move |_, window, cx| {
                                let order = save.borrow().clone();
                                let state = view.read(cx).state.clone();
                                let result = serde_json::to_string(&order)
                                    .map_err(|error| error.to_string())
                                    .and_then(|raw| {
                                        calmmd_core::kv::write(state.read(cx).context(), KEY, &raw)
                                    });
                                if let Err(error) = result {
                                    state.update(cx, |state, cx| state.set_message(error, cx));
                                    return;
                                }
                                view.update(cx, |view, cx| {
                                    view.toolbar_items = order;
                                    cx.notify();
                                });
                                window.close_dialog(cx);
                            }),
                    ),
            )
    });
}
