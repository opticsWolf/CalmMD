//! Rendered document modes. A single source block can be edited in place while the rest stays rendered.
use super::visual_edit::{changed_range, Projection};
use super::*;
use gpui_kit::base::text::{
    editor_host::{parse_markdown, BlockNode},
    MarkdownExtensions, TextView, TextViewState,
};
use gpui_kit::base::ElementExt as _;
use gpui_kit::{
    assets::IconName,
    component::input::{
        EditorState, InputEdit, InputEditsEvent, TextDecoration, TextDecorationCollection,
    },
};
use std::ops::Range;

struct BlockEdit {
    input: Entity<EditorState>,
    path: String,
    source: SharedString,
    range: Range<usize>,
    prefix: &'static str,
    visual: Option<Projection>,
    decorations: TextDecorationCollection,
    _changes: Subscription,
    _selection: Subscription,
}

pub(super) struct DocumentView {
    state: Entity<WorkspaceState>,
    source_editor: Entity<EditorPanel>,
    platform: Arc<dyn Platform>,
    mode: ViewMode,
    active: Option<BlockEdit>,
    parsed_source: SharedString,
    parsed_path: Option<String>,
    rendered: Vec<Entity<TextViewState>>,
    blocks: Vec<Range<usize>>,
    references: String,
    zoom: f32,
    page_width: Option<Pixels>,
}

impl DocumentView {
    pub fn new(
        state: Entity<WorkspaceState>,
        source_editor: Entity<EditorPanel>,
        platform: Arc<dyn Platform>,
    ) -> Self {
        Self {
            state,
            source_editor,
            platform,
            mode: ViewMode::Live,
            active: None,
            parsed_source: "".into(),
            parsed_path: None,
            rendered: Vec::new(),
            blocks: Vec::new(),
            references: String::new(),
            zoom: 1.0,
            page_width: None,
        }
    }

    pub fn set_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        self.active = None;
        self.mode = mode;
        cx.notify();
    }

    pub fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        self.zoom = zoom;
        cx.notify();
    }

    pub fn input(&self, cx: &App) -> Option<Entity<EditorState>> {
        let edit = self.active.as_ref()?;
        let state = self.state.read(cx);
        (state.active() == Some(edit.path.as_str()) && state.text() == edit.source.as_ref())
            .then(|| edit.input.clone())
    }

    fn begin_edit(
        &mut self,
        range: Range<usize>,
        append: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let Some(path) = state.active().map(str::to_owned) else {
            return;
        };
        let source: SharedString = state.text().to_owned().into();
        let Some(value) = source.get(range.clone()) else {
            return;
        };
        let prefix = if !append || source.trim().is_empty() || source.ends_with("\n\n") {
            ""
        } else if source.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        let visual =
            (self.mode == ViewMode::Preview).then(|| Projection::new(value, &self.references));
        if visual
            .as_ref()
            .is_some_and(|visual| !append && !visual.is_editable())
        {
            return;
        }
        let editable = visual.as_ref().map_or(value, |visual| visual.text.as_str());
        let input = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .line_number(false)
                .folding(false)
                .default_value(editable);
            if visual.is_some() {
                editor = editor.soft_wrap(true).scroll_beyond_last_line(Some(0));
                editor.set_scrollbar_renderer(Some(std::rc::Rc::new(|_, _, _| {
                    div().into_any_element()
                })));
            }
            editor
        });
        let selection = cx.observe(&input, |_, _, cx| cx.notify());
        let decorations = input.update(cx, |input, cx| {
            input.set_selected_range(editable.len()..editable.len(), cx);
            input.create_decorations_collection(Vec::new(), cx)
        });
        if let Some(visual) = &visual {
            set_decorations(&decorations, visual, cx);
        }
        let handle = window.window_handle();
        let owner = cx.entity().downgrade();
        let changes = cx.subscribe(&input, move |_, input, event: &InputEditsEvent, cx| {
            let owner = owner.clone();
            let change = match event.edits() {
                [(edit, _)] => Some(*edit),
                _ => None,
            };
            after_window_action(cx, handle, move |window, cx| {
                if let Some(owner) = owner.upgrade() {
                    owner.update(cx, |view, cx| {
                        view.apply_input_edit(&input, change, window, cx)
                    });
                }
            });
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
        self.active = Some(BlockEdit {
            input,
            path,
            source,
            range,
            prefix,
            visual,
            decorations,
            _changes: changes,
            _selection: selection,
        });
        cx.notify();
    }

    pub fn apply_edit(
        &mut self,
        input: &Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_input_edit(input, None, window, cx);
    }

    fn apply_input_edit(
        &mut self,
        input: &Entity<EditorState>,
        change: Option<InputEdit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.active.as_mut().filter(|edit| &edit.input == input) else {
            return;
        };
        let state = self.state.read(cx);
        if state.active() != Some(edit.path.as_str()) || state.text() != edit.source.as_ref() {
            return;
        }
        let value = input.read(cx).value();
        let original = &edit.source[edit.range.clone()];
        let markdown = if let Some(visual) = &edit.visual {
            if visual.text == value.as_ref() {
                return;
            }
            let Some(markdown) = visual.replace(original, &value, change) else {
                input.update(cx, |input, cx| {
                    input.set_value(visual.text.as_str(), window, cx)
                });
                return;
            };
            markdown
        } else if original != value.as_ref() {
            value.to_string()
        } else {
            return;
        };
        let replacement = format!("{}{markdown}", edit.prefix);
        let text = replace_source(
            &self.state,
            &self.source_editor,
            &edit.path,
            &edit.source,
            edit.range.clone(),
            &replacement,
            window,
            cx,
        );
        if let Some(text) = text {
            edit.range.start += edit.prefix.len();
            edit.range.end = edit.range.start + markdown.len();
            edit.prefix = "";
            edit.source = text;
            if edit.visual.is_some() {
                let selection = input.read(cx).selected_range();
                let visual = Projection::new(&markdown, &self.references);
                if visual.text != value.as_ref() {
                    set_visual_input(input, &visual.text, selection, window, cx);
                }
                set_decorations(&edit.decorations, &visual, cx);
                edit.visual = Some(visual);
            }
            cx.notify();
        }
    }

    pub fn visual_selection(&self, cx: &App) -> Option<Range<usize>> {
        let state = self.state.read(cx);
        if let Some(input) = self.input(cx) {
            let edit = self.active.as_ref()?;
            let visual = edit.visual.as_ref()?;
            let selection = visual.source_selection(
                &edit.source[edit.range.clone()],
                input.read(cx).selected_range(),
            )?;
            return Some(edit.range.start + selection.start..edit.range.start + selection.end);
        }
        if self.mode != ViewMode::Preview
            || self.parsed_path.as_deref() != state.active()
            || self.parsed_source.as_ref() != state.text()
        {
            return None;
        }
        self.blocks
            .iter()
            .zip(&self.rendered)
            .filter_map(|(block, rendered)| {
                let selection = rendered.read(cx).selected_source_range()?;
                let end = selection.end.min(block.len());
                (selection.start < end).then_some(block.start + selection.start..block.start + end)
            })
            .reduce(|left, right| left.start.min(right.start)..left.end.max(right.end))
    }

    /// The formatting tools operate on the source editor, then refresh this visual projection.
    pub fn source_command(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.active.as_mut().filter(|edit| edit.visual.is_some()) else {
            return;
        };
        if edit.source.as_ref() == text {
            return;
        }
        let (old, new) = changed_range(&edit.source, text);
        if old.start < edit.range.start || old.end > edit.range.end {
            self.active = None;
            cx.notify();
            return;
        }
        let selection = edit.input.read(cx).selected_range();
        edit.range.end = edit.range.end - old.len() + new.len();
        edit.source = text.to_owned().into();
        let visual = Projection::new(&text[edit.range.clone()], &self.references);
        if edit.input.read(cx).value().as_ref() != visual.text {
            set_visual_input(&edit.input, &visual.text, selection, window, cx);
        }
        set_decorations(&edit.decorations, &visual, cx);
        edit.visual = Some(visual);
        cx.notify();
    }

    pub fn edit_line(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state
            .text()
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>();
        let range = self
            .blocks
            .iter()
            .find(|range| range.end >= offset)
            .cloned();
        if let Some(range) = range {
            self.begin_edit(range, false, window, cx);
        }
    }

    pub fn finish_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit) = &self.active {
            self.apply_edit(&edit.input.clone(), window, cx);
        }
        self.active = None;
        cx.notify();
    }

    fn render_editor(
        &self,
        edit: &BlockEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = Theme::global(cx).accent;
        let text = edit.input.read(cx).value();
        let visual = edit.visual.as_ref();
        let size = visual
            .and_then(|visual| visual.heading)
            .map_or(18., |level| match level {
                1 => 32.,
                2 => 26.,
                3 => 22.,
                _ => 18.,
            })
            * self.zoom;
        let height = if visual.is_some() {
            let width = self
                .page_width
                .unwrap_or_else(|| window.viewport_size().width.min(px(840. * self.zoom)));
            let available = (width - px(128. * self.zoom + 24.)).max(px(24.));
            let font = Font {
                weight: FontWeight::BOLD,
                ..font("Georgia")
            };
            let runs = [TextRun {
                len: text.len(),
                font,
                color: Theme::global(cx).foreground,
                ..Default::default()
            }];
            let rows = window
                .text_system()
                .shape_text(text.clone(), px(size), &runs, Some(available), None)
                .map(|lines| {
                    lines
                        .iter()
                        .map(|line| 1 + line.wrap_boundaries().len())
                        .sum::<usize>()
                })
                .unwrap_or_else(|_| text.split('\n').count())
                .max(text.split('\n').count());
            px(size * 1.5).ceil() * rows as f32 + px(12.)
        } else {
            let rows = text
                .split('\n')
                .map(|line| line.chars().count().max(1).div_ceil(72))
                .sum::<usize>()
                .clamp(2, 24);
            px(size * 1.52 * rows as f32 + 10.)
        };
        let editor = Editor::new(&edit.input).text_size(px(size)).h(height);
        div()
            .id("active-document-block")
            .v_flex()
            .gap_1()
            .w_full()
            .key_context("DocumentBlock")
            .on_action(
                cx.listener(|this, _: &FinishBlock, window, cx| this.finish_edit(window, cx)),
            )
            .when(visual.is_none(), |pane| {
                pane.border_l_2().border_color(accent).pl_2()
            })
            .child(editor.when(visual.is_some(), |editor| {
                editor
                    .appearance(false)
                    .bordered(false)
                    .font_family("Georgia")
                    .when(
                        visual.is_some_and(|visual| visual.heading.is_some()),
                        |editor| editor.font_weight(FontWeight::BOLD),
                    )
            }))
            .when(visual.is_none(), |pane| {
                pane.child(
                    div().h_flex().justify_end().child(
                        Button::new("finish-block")
                            .label(tr(cx, "Done"))
                            .ghost()
                            .compact()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.finish_edit(window, cx)),
                            ),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_block(
        &self,
        index: usize,
        range: Range<usize>,
        source: &SharedString,
        path: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.clone();
        let task_state = self.state.clone();
        let editor = self.source_editor.clone();
        let expected = source.clone();
        let task_path = path.to_owned();
        let offset = range.start;
        let markdown = TextView::new(&self.rendered[index])
            .markdown_extensions(preview::extensions(state.clone(), self.platform.clone()))
            .selectable(true)
            .on_link_click(move |url, _, _, cx| {
                state.update(cx, |state, cx| state.open_link(url, cx));
            })
            .on_task_toggle(move |event, window, cx| {
                let marker = event.marker_range();
                let marker = offset + marker.start..offset + marker.end;
                if let Some(replacement) =
                    task_replacement(&expected, marker.clone(), event.checked())
                {
                    replace_source(
                        &task_state,
                        &editor,
                        &task_path,
                        &expected,
                        marker,
                        replacement,
                        window,
                        cx,
                    );
                }
            });
        let click_range = range.clone();
        let visual = self.mode == ViewMode::Preview;
        let edit = Button::new(("edit-block", index))
            .icon(IconName::Pencil)
            .ghost()
            .compact()
            .accessibility_label(tr(cx, "Edit block"))
            .tooltip(tr(cx, "Edit block"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.begin_edit(range.clone(), false, window, cx)
            }));
        div()
            .id(("document-block", index))
            .h_flex()
            .items_start()
            .gap_1()
            .w_full()
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_w(px(0.))
                    .child(markdown)
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if visual || event.click_count() == 2 {
                            this.begin_edit(click_range.clone(), false, window, cx);
                        }
                    })),
            )
            .when(!visual, |block| block.child(edit))
            .into_any_element()
    }
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = calmmd, no_json)]
pub(super) struct FinishBlock;

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let source: SharedString = state.text().to_owned().into();
        let path = state.active().map(str::to_owned);
        if self
            .active
            .as_ref()
            .is_some_and(|edit| path.as_deref() != Some(&edit.path) || edit.source != source)
        {
            self.active = None;
        }
        if self.parsed_source != source || self.parsed_path != path {
            let (blocks, references) = document_blocks(&source);
            let reuse = self.parsed_path == path && self.references == references;
            self.rendered = blocks
                .iter()
                .enumerate()
                .map(|(index, range)| {
                    let text = &source[range.clone()];
                    if reuse
                        && self
                            .blocks
                            .get(index)
                            .is_some_and(|old| self.parsed_source.get(old.clone()) == Some(text))
                    {
                        return self.rendered[index].clone();
                    }
                    let markdown = format!("{text}{references}");
                    cx.new(|cx| TextViewState::markdown(&markdown, cx))
                })
                .collect();
            self.blocks = blocks;
            self.references = references;
            self.parsed_source = source.clone();
            self.parsed_path = path.clone();
        }

        let theme = Theme::global(cx);
        let (border, background, desk) = (theme.border, theme.background, theme.muted);
        let paper = self.mode == ViewMode::Preview;
        let observed_width = self.page_width;
        let owner = cx.entity().downgrade();
        let mut page = div()
            .id("document-paper")
            .v_flex()
            .gap_2()
            .w_full()
            .max_w(px(if paper { 840. } else { 930. } * self.zoom))
            .px(px(if paper { 64. } else { 30. }))
            .py(px(30.))
            .font_family("Georgia")
            .text_size(px(18. * self.zoom))
            .when(paper, |page| {
                page.min_h(px(880.))
                    .border_1()
                    .border_color(border)
                    .bg(background)
                    .on_prepaint(move |bounds, window, cx| {
                        let width = bounds.size.width;
                        if observed_width != Some(width) {
                            let owner = owner.clone();
                            window.defer(cx, move |_, cx| {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |view, cx| {
                                        if view.page_width != Some(width) {
                                            view.page_width = Some(width);
                                            cx.notify();
                                        }
                                    });
                                }
                            });
                        }
                    })
            });
        if let Some(path) = &path {
            // Keep the edited span as one block even when typing adds blank lines or headings.
            let mut inserted = false;
            for (index, range) in self.blocks.iter().enumerate() {
                if let Some(edit) = &self.active {
                    if !inserted && range.end > edit.range.start {
                        page = page.child(self.render_editor(edit, window, cx));
                        inserted = true;
                    }
                    if range.start < edit.range.end && range.end > edit.range.start {
                        continue;
                    }
                    // A newly typed block can have joined the previous block at a deleted boundary.
                    if inserted && edit.range.is_empty() && range.contains(&edit.range.start) {
                        continue;
                    }
                }
                page = page.child(self.render_block(index, range.clone(), &source, path, cx));
            }
            if let Some(edit) = &self.active {
                if !inserted {
                    page = page.child(self.render_editor(edit, window, cx));
                }
            } else {
                let end = source.len();
                if paper {
                    page = page.child(
                        div()
                            .id("blank-paragraph")
                            .flex_1()
                            .min_h(px(40.))
                            .cursor_text()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.begin_edit(end..end, true, window, cx)
                            })),
                    );
                } else {
                    page = page.child(
                        div().pt_3().child(
                            Button::new("add-paragraph")
                                .icon(IconName::Plus)
                                .label(tr(cx, "Add paragraph"))
                                .ghost()
                                .compact()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.begin_edit(end..end, true, window, cx)
                                })),
                        ),
                    );
                }
            }
        } else {
            page = page.child(Label::new(tr(
                cx,
                "Select a document, or create a new one.",
            )));
        }
        div()
            .id("document-scroll")
            .size_full()
            .overflow_y_scroll()
            .when(paper, |scroll| scroll.bg(desk))
            .child(
                div()
                    .h_flex()
                    .justify_center()
                    .px(px(14.))
                    .py(px(16.))
                    .child(page),
            )
    }
}

fn set_decorations(collection: &TextDecorationCollection, projection: &Projection, cx: &mut App) {
    let link_color = Theme::global(cx).link;
    collection.set(
        projection
            .decorations(link_color)
            .map(|(range, style)| TextDecoration::new(range, style))
            .collect(),
        cx,
    );
}

fn set_visual_input(
    input: &Entity<EditorState>,
    text: &str,
    range: Range<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let boundary = |offset| {
        let mut offset = text.len().min(offset);
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    input.update(cx, |input, cx| {
        input.set_value(text, window, cx);
        input.set_selected_range(boundary(range.start)..boundary(range.end), cx);
    });
}

fn replace_source(
    state: &Entity<WorkspaceState>,
    source_editor: &Entity<EditorPanel>,
    path: &str,
    expected: &str,
    range: Range<usize>,
    replacement: &str,
    window: &mut Window,
    cx: &mut App,
) -> Option<SharedString> {
    let current = state.read(cx);
    if current.active() != Some(path)
        || current.text() != expected
        || expected.get(range.clone()).is_none()
    {
        return None;
    }
    let input = source_editor.update(cx, |panel, cx| {
        panel.editor.sync_text(Some(path), expected, window, cx);
        panel.editor.input.clone()
    });
    let value = input.update(cx, |input, cx| {
        input.set_selected_range(range, cx);
        input.replace(replacement, window, cx);
        input.value()
    });
    state.update(cx, |state, cx| state.editor_changed(value.to_string(), cx));
    Some(value)
}

fn task_replacement(source: &str, marker: Range<usize>, checked: bool) -> Option<&'static str> {
    match (source.get(marker), checked) {
        (Some("[ ]"), true) => Some("[x]"),
        (Some("[x]" | "[X]"), false) => Some("[ ]"),
        _ => None,
    }
}

fn document_blocks(source: &str) -> (Vec<Range<usize>>, String) {
    let extensions = MarkdownExtensions::default().frontmatter();
    let Ok(blocks) = parse_markdown(source, Arc::new(extensions)) else {
        let fallback = (!source.trim().is_empty())
            .then_some(0..source.len())
            .into_iter()
            .collect();
        return (fallback, String::new());
    };

    let mut ranges = Vec::new();
    let mut references = String::new();
    for block in blocks {
        let Some(span) = block.span() else { continue };
        let range = span.start..span.end;
        let Some(text) = source.get(range.clone()) else {
            continue;
        };
        if matches!(block, BlockNode::Definition { .. }) {
            // Definitions render no content, but every rendered slice needs the document's links.
            references.push_str("\n\n");
            references.push_str(text);
        } else if !text.trim().is_empty() {
            let end = range.start + text.trim_end_matches(['\r', '\n']).len();
            ranges.push(range.start..end);
        }
    }
    (ranges, references)
}

#[cfg(test)]
mod tests {
    use super::{document_blocks, task_replacement};

    #[test]
    fn rendered_blocks_preserve_byte_ranges_and_fenced_content() {
        let source =
            "# Grüße\n\n- [ ] Ärger\n  - [X] Kind\n\n~~~\n# Not a heading\n~~~\n\nLast paragraph\n";
        let blocks: Vec<_> = document_blocks(source)
            .0
            .into_iter()
            .map(|range| &source[range])
            .collect();
        assert_eq!(
            blocks,
            [
                "# Grüße",
                "- [ ] Ärger\n  - [X] Kind",
                "~~~\n# Not a heading\n~~~",
                "Last paragraph"
            ]
        );
    }

    #[test]
    fn reference_links_keep_their_definitions_across_blocks() {
        let source = "See [one].\n\nSee [two].\n\n[one]: /one.md\n[two]: /two.md";
        let (ranges, definitions) = document_blocks(source);
        assert_eq!(ranges.len(), 2);
        assert!(definitions.contains("[one]: /one.md"));
        assert!(definitions.contains("[two]: /two.md"));
    }

    #[test]
    fn stale_or_non_task_markers_are_ignored() {
        let source = "- [ ] first\n- [X] second";
        assert_eq!(task_replacement(source, 2..5, true), Some("[x]"));
        assert_eq!(task_replacement(source, 14..17, false), Some("[ ]"));
        assert_eq!(task_replacement(source, 2..5, false), None);
        assert_eq!(task_replacement(source, 0..3, true), None);
    }
}
