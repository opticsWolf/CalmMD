//! GPUI Kit shell: root view, dock panels and command actions.

use crate::{
    editor::MarkdownEditor,
    platform::Platform,
    preview,
    services::Services,
    state::{CoreEvents, WorkspaceState},
    theme,
};
use gpui_kit::{
    component::{
        button::Button,
        dock::{BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent},
        input::{Editor, Input, InputState},
        label::Label,
    },
    *,
};
use calmmd_core::WindowRegistry;
use gpui_kit::base::StyledExt;
use std::sync::{Arc, Mutex};

gpui_kit::actions!(
    calmmd,
    [
        OpenFolder,
        NewDocument,
        SaveDocument,
        NewWindow,
        CloseWindow,
        NextWindow,
        UndoDocument,
        RedoDocument,
        KeepMine,
        LoadTheirs,
        ToggleTheme,
        Quit
    ]
);

struct WindowEntry {
    handle: AnyWindowHandle,
    view: Entity<AppView>,
}

#[derive(Clone)]
struct WindowList(Arc<Mutex<Vec<WindowEntry>>>);

#[allow(dead_code)]
/// Registers actions/key bindings and opens the first shell window.
pub fn init(
    cx: &mut App,
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    platform: Arc<dyn Platform>,
) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", OpenFolder, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-o", OpenFolder, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-n", NewDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-n", NewDocument, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveDocument, None),
    ]);

    let windows = WindowList(Arc::new(Mutex::new(Vec::new())));
    let open_services = services.clone();
    let open_events = events.clone();
    let open_platform = platform.clone();
    let open_windows = windows.clone();
    let action_open_windows = open_windows.clone();
    cx.on_action(move |_: &OpenFolder, cx| {
        if let Some(entry) = active_entry(&action_open_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| view.open_folder(cx));
        }
    });

    let save_windows = windows.clone();
    cx.on_action(move |_: &SaveDocument, cx| {
        if let Some(entry) = active_entry(&save_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.save_active(cx));
            });
        }
    });

    let new_doc_windows = windows.clone();
    cx.on_action(move |_: &NewDocument, cx| {
        if let Some(entry) = active_entry(&new_doc_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.create_document("Untitled.md", cx));
            });
        }
    });

    let new_services = services.clone();
    let new_events = events.clone();
    let new_platform = platform.clone();
    let new_windows = windows.clone();
    cx.on_action(move |_: &NewWindow, cx| {
        open_shell_window(
            cx,
            new_services.clone(),
            new_events.clone(),
            new_platform.clone(),
            new_windows.clone(),
        );
    });

    let cycle_windows = windows.clone();
    cx.on_action(move |_: &NextWindow, cx| {
        let handles: Vec<AnyWindowHandle> = cycle_windows
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.handle)
            .collect();
        if handles.is_empty() {
            return;
        }
        let active = cx.active_window().map(|handle| handle.window_id());
        let next = handles
            .iter()
            .find(|handle| Some(handle.window_id()) == active)
            .and_then(|current| {
                handles
                    .iter()
                    .cycle()
                    .skip_while(|handle| handle.window_id() != current.window_id())
                    .nth(1)
                    .copied()
            })
            .unwrap_or(handles[0]);
        let _ = cx.update_window(next, |_, window, _| window.activate_window());
    });

    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    let undo_windows = windows.clone();
    cx.on_action(move |_: &UndoDocument, cx| {
        if let Some(entry) = active_entry(&undo_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.undo_active(cx));
            });
        }
    });
    let redo_windows = windows.clone();
    cx.on_action(move |_: &RedoDocument, cx| {
        if let Some(entry) = active_entry(&redo_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.redo_active(cx));
            });
        }
    });
    let keep_windows = windows.clone();
    cx.on_action(move |_: &KeepMine, cx| {
        if let Some(entry) = active_entry(&keep_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.resolve_conflict(true, cx));
            });
        }
    });
    let theirs_windows = windows.clone();
    cx.on_action(move |_: &LoadTheirs, cx| {
        if let Some(entry) = active_entry(&theirs_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.resolve_conflict(false, cx));
            });
        }
    });
    cx.on_action(|_: &ToggleTheme, cx| theme::toggle(cx));
    cx.bind_keys([
        KeyBinding::new("ctrl-alt-z", UndoDocument, None),
        KeyBinding::new("ctrl-alt-shift-z", RedoDocument, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-t", ToggleTheme, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-t", ToggleTheme, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());

    open_shell_window(cx, open_services, open_events, open_platform, open_windows);
}

fn open_shell_window(
    cx: &mut App,
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    platform: Arc<dyn Platform>,
    windows: WindowList,
) {
    let label = if services.windows.windows().is_empty() {
        "main".to_owned()
    } else {
        format!("window-{}", services.windows.windows().len() + 1)
    };
    services.windows.register(&label);

    let (handle, view) = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        cx.new(|cx| {
            AppView::new(
                window,
                cx,
                services.clone(),
                events.clone(),
                platform.clone(),
                label.clone(),
            )
        })
    })
    .expect("failed to open CalmMD window");
    windows.0.lock().unwrap().push(WindowEntry { handle, view });
}

/// Root view of one GPUI window.
pub struct AppView {
    state: Entity<WorkspaceState>,
    dock: Entity<DockArea>,
    platform: Arc<dyn Platform>,
    #[allow(dead_code)]
    window_label: String,
    #[allow(dead_code)]
    services: Arc<Services>,
    #[allow(dead_code)]
    events: Entity<CoreEvents>,
}

impl AppView {
    fn new(
        window: &mut Window,
        cx: &mut App,
        services: Arc<Services>,
        events: Entity<CoreEvents>,
        platform: Arc<dyn Platform>,
        window_label: String,
    ) -> Self {
        let state = cx.new(|cx| WorkspaceState::new(services.clone(), events.clone(), window_label.clone(), cx));
        let sidebar = cx.new(|cx| SidebarPanel::new(state.clone(), platform.clone(), window, cx));
        let editor = cx.new(|cx| EditorPanel::new(state.clone(), window, cx));
        let preview = cx.new(|cx| PreviewPanel::new(state.clone(), platform.clone(), cx));
        let status = cx.new(|cx| StatusPanel::new(state.clone(), events.clone(), cx));

        let dock = cx.new(|cx| {
            DockArea::new(format!("calmmd-{window_label}"), Some(1), window, cx)
                .with_renderer(DockSkin::new(cx))
        });
        dock.update(cx, |dock, cx| {
            dock.set_center(
                DockLayout::h_split()
                    .child(DockLayout::tabs().panel(sidebar), Some(px(240.0)))
                    .child(
                        DockLayout::v_split()
                            .child(
                                DockLayout::h_split()
                                    .child(DockLayout::tabs().panel(editor), None)
                                    .child(DockLayout::tabs().panel(preview), None),
                                None,
                            ),
                        None,
                    ),
                window,
                cx,
            );
            dock.set_dock(
                DockPlacement::Bottom,
                DockLayout::tabs().panel(status),
                window,
                cx,
            );
            dock.set_dock_size(DockPlacement::Bottom, px(32.0), window, cx);
        });

        Self {
            state,
            dock,
            platform,
            window_label,
            services,
            events,
        }
    }

    fn open_folder(&mut self, cx: &mut App) {
        let platform = self.platform.clone();
        let state = self.state.clone();
        cx.spawn(async move |cx| {
            let Some(path) = platform.open_folder() else { return };
            let _ = state.update(cx, |state, cx| state.open_folder(path, cx));
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock.clone())
    }
}

fn active_entry(windows: &WindowList, cx: &App) -> Option<WindowEntry> {
    let active = cx.active_window()?.window_id();
    windows
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.handle.window_id() == active)
        .map(|entry| WindowEntry {
            handle: entry.handle,
            view: entry.view.clone(),
        })
}

fn panel_focus(cx: &App) -> FocusHandle {
    cx.focus_handle()
}

struct SidebarPanel {
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
    search: Entity<InputState>,
    focus: FocusHandle,
    _search_subscription: Subscription,
}

impl SidebarPanel {
    fn new(
        state: Entity<WorkspaceState>,
        platform: Arc<dyn Platform>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes"));
        let search_state = search.clone();
        let state_for_search = state.clone();
        let search_subscription = cx.subscribe(&search, move |_, event: &gpui_kit::component::input::InputEvent, cx| {
            if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                let query = search_state.read(cx).value().to_string();
                state_for_search.update(cx, |state, cx| state.set_search_query(query, cx));
            }
        });
        Self {
            state,
            platform,
            search,
            focus: panel_focus(cx),
            _search_subscription: search_subscription,
        }
    }
}

impl EventEmitter<PanelEvent> for SidebarPanel {}
impl Focusable for SidebarPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for SidebarPanel {
    fn panel_name(&self) ->  &'static str {
        "calmmd-sidebar".into()
    }
}

impl Panel for SidebarPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Label::new("Files")
    }
}

impl Render for SidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let mut column = div().v_flex().gap(px(6.0)).p(px(10.0)).size_full();
        column = column.child(
            Button::new("open-folder")
                .label("Open folder")
                .on_click(cx.listener(|this, _, _window, cx| {
                    let platform = this.platform.clone();
                    let state = this.state.clone();
                    cx.spawn(async move |_, cx| {
                        if let Some(path) = platform.open_folder() {
                            let _ = state.update(cx, |state, cx| state.open_folder(path, cx));
                        }
                    })
                    .detach();
                })),
        );
        column = column.child(
            Button::new("new-document")
                .label("New document")
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.state
                        .update(cx, |state, cx| state.create_document("Untitled.md", cx));
                })),
        );
        column = column.child(
            Button::new("save-document")
                .label("Save")
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.state.update(cx, |state, cx| state.save_active(cx));
                })),
        );
        column = column.child(
            Button::new("undo-document")
                .label("Undo tool edit")
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.state.update(cx, |state, cx| state.undo_active(cx));
                })),
        );
        column = column.child(
            Button::new("redo-document")
                .label("Redo tool edit")
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.state.update(cx, |state, cx| state.redo_active(cx));
                })),
        );
        column = column.child(Input::new(&self.search));
        for hit in state.search_results() {
            let path = hit.path.clone();
            let state = self.state.clone();
            column = column.child(
                Button::new(ElementId::Name(format!("search-hit:{path}:{}", hit.line).into()))
                    .label(format!("{}:{} {}", hit.path.rsplit(['/', '\\']).next().unwrap_or(&hit.path), hit.line + 1, hit.text.trim()))
                    .on_click(move |_, _window, cx| {
                        state.update(cx, |state, cx| state.open_document(&path, cx));
                    }),
            );
        }
        column = column.child(
            Button::new("new-window")
                .label("New window")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(NewWindow), cx)),
        );
        column = column.child(
            Button::new("next-window")
                .label("Next window")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(NextWindow), cx)),
        );
        column = column.child(
            Button::new("close-window")
                .label("Close window")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(CloseWindow), cx)),
        );
        column = column.child(Label::new(
            state
                .root()
                .map(|path| format!("Root: {path}"))
                .unwrap_or_else(|| "No folder open".into()),
        ));
        for document in state.documents() {
            let path = document.path.clone();
            let state = self.state.clone();
            column = column.child(
                Button::new(ElementId::Name(format!("open-document:{path}").into()))
                    .label(document.name.clone())
                    .on_click(move |_, _window, cx| {
                        state.update(cx, |state, cx| state.open_document(&path, cx));
                    }),
            );
        }
        column
    }
}

struct EditorPanel {
    state: Entity<WorkspaceState>,
    editor: MarkdownEditor,
}

impl EditorPanel {
    fn new(state: Entity<WorkspaceState>, window: &mut Window, cx: &mut App) -> Self {
        let text = state.read(cx).text().to_owned();
        let editor = MarkdownEditor::new(window, cx, state.clone(), &text);
        Self { state, editor }
    }
}

impl EventEmitter<PanelEvent> for EditorPanel {}
impl Focusable for EditorPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.input.read(cx).focus_handle(cx)
    }
}
impl BasePanel for EditorPanel {
    fn panel_name(&self) ->  &'static str {
        "calmmd-editor".into()
    }
}

impl Panel for EditorPanel {
    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Label::new(self.state.read(cx).title())
    }
}

impl Render for EditorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, language, query, context) = {
            let state = self.state.read(cx);
            (
                state.text().to_owned(),
                state.spell_language().map(str::to_owned),
                state.search_query().to_owned(),
                state.context().clone(),
            )
        };
        self.editor.sync_text(&text, window, cx);
        self.editor.refresh_decorations(&context, &text, language.as_deref(), &query, cx);
        div()
            .v_flex()
            .size_full()
            .child(Editor::new(&self.editor.input).h(gpui_kit::relative(1.0)))
    }
}

struct PreviewPanel {
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
    focus: FocusHandle,
}

impl PreviewPanel {
    fn new(state: Entity<WorkspaceState>, platform: Arc<dyn Platform>, cx: &mut App) -> Self {
        Self {
            state,
            platform,
            focus: panel_focus(cx),
        }
    }
}

impl EventEmitter<PanelEvent> for PreviewPanel {}
impl Focusable for PreviewPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for PreviewPanel {
    fn panel_name(&self) ->  &'static str {
        "calmmd-preview".into()
    }
}

impl Panel for PreviewPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Label::new("Preview")
    }
}

impl Render for PreviewPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.state.read(cx).text().to_owned();
        let state = self.state.clone();
        let view = preview::view(&text, state.clone(), self.platform.clone())
            .on_link_click(move |url, _, _window, cx| {
                let target = url.to_string();
                state.update(cx, |state, cx| state.open_link(&target, cx));
            });
        let _ = window;
        div().size_full().p(px(12.0)).child(view)
    }
}

struct StatusPanel {
    state: Entity<WorkspaceState>,
    events: Entity<CoreEvents>,
    focus: FocusHandle,
}

impl StatusPanel {
    fn new(
        state: Entity<WorkspaceState>,
        events: Entity<CoreEvents>,
        cx: &mut App,
    ) -> Self {
        Self {
            state,
            events,
            focus: panel_focus(cx),
        }
    }
}

impl EventEmitter<PanelEvent> for StatusPanel {}
impl Focusable for StatusPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for StatusPanel {
    fn panel_name(&self) ->  &'static str {
        "calmmd-status".into()
    }
}

impl Panel for StatusPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Label::new("Status")
    }
}

impl Render for StatusPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let events = self.events.read(cx);
        let mut row = div()
            .h_flex()
            .items_center()
            .gap(px(12.0))
            .px(px(10.0))
            .child(Label::new(state.message().to_owned()))
            .child(Label::new(format!("autosave: {}", state.autosave_status())))
            .child(Label::new(format!("spell: {}", state.spell_language().unwrap_or("en-US"))))
            .child(Label::new(format!("window: {}", state.window_label())))
            .child(Label::new(format!("core events: {}", events.recent().len())))
            .child(
                Button::new("cycle-spell")
                    .label("Language")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.state.update(cx, |state, cx| state.cycle_spell_language(cx));
                    })),
            )
            .child(
                Button::new("toggle-theme")
                    .label("Light / dark")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleTheme), cx)),
            );
        if state.conflict() {
            row = row
                .child(Button::new("keep-mine").label("Keep mine").on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(KeepMine), cx);
                }))
                .child(Button::new("load-theirs").label("Load theirs").on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(LoadTheirs), cx);
                }));
        }
        row
    }
}
