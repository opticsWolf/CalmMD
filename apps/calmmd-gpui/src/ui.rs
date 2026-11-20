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
        WindowExt as _,
        button::{Button, ButtonVariants},
        dock::{BasePanel, Panel, PanelEvent},
        global_state::GlobalState,
        input::{Editor, Input, InputState},
        label::Label,
        menu::AppMenuBar,
        native_menu::NativeMenu,
        resizable::{h_resizable, resizable_panel},
        theme::Theme,
    },
    *,
};
use serde_json::Value;
use calmmd_core::WindowRegistry;
use gpui_kit::{base::StyledExt, prelude::FluentBuilder};
use std::{collections::HashSet, path::Path, sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}}};

gpui_kit::actions!(
    calmmd,
    [
        OpenFolder,
        RecentFolders,
        ParentFolder,
        CloseFolder,
        ReopenDocument,
        Spaces,
        ToggleSidebar,
        ToggleToolbar,
        CustomizeToolbar,
        GoToLine,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ImageLink,
        ImageFile,
        ImageLocation,
        InsertFootnote,
        InsertMermaid,
        OrderedList,
        ChooseHeading,
        HelpShortcuts,
        HelpMarkdown,
        HelpWelcome,
        CommandPalette,
        FilePalette,
        HeadingPalette,
        ToggleSpelling,
        ToggleHighlight,
        Appearance,
        OpenFile,
        NewDocument,
        SaveDocument,
        SaveAs,
        CloseDocument,
        SaveVersion,
        RevealDocument,
        ExportHtml,
        ExportPdf,
        CopyHtml,
        StartMcp,
        StopMcp,
        NewWindow,
        CloseWindow,
        NextWindow,
        UndoDocument,
        RedoDocument,
        KeepMine,
        LoadTheirs,
        CopyMarkdown,
        EditorCut,
        EditorCopy,
        EditorPaste,
        EditorSelectAll,
        EditorFind,
        FormatBold,
        FormatItalic,
        FormatStrike,
        FormatCode,
        FormatLink,
        FormatHeading,
        FormatList,
        FormatTask,
        FormatQuote,
        InsertCodeBlock,
        InsertMath,
        InsertTable,
        InsertRule,
        ShowFiles,
        ShowOutline,
        ShowSearch,
        ShowHistory,
        ViewSource,
        ViewSplit,
        ViewPreview,
        ChooseLanguage,
        About,
        ToggleTheme,
        Quit
    ]
);

mod parity;
mod table;
mod toolbar;

fn install_app_menus(cx: &mut App) {
    GlobalState::global_mut(cx).set_app_menus(app_menus().into_iter().map(Menu::owned).collect());
    cx.set_menus(app_menus());
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("File").items([
            MenuItem::action("New Window", NewWindow),
            MenuItem::action("New Document", NewDocument),
            MenuItem::action("Open…", OpenFile),
            MenuItem::action("Open Folder…", OpenFolder),
            MenuItem::action("Open Recent Folder", RecentFolders),
            MenuItem::action("Open Parent Folder", ParentFolder),
            MenuItem::action("Spaces", Spaces),
            MenuItem::separator(),
            MenuItem::action("Save", SaveDocument),
            MenuItem::action("Save a Copy…", SaveAs),
            MenuItem::action("Save Version...", SaveVersion),
            MenuItem::action("Version History", ShowHistory),
            MenuItem::action("Close Document", CloseDocument),
            MenuItem::action("Reopen Closed Document", ReopenDocument),
            MenuItem::action("Close Folder", CloseFolder),
            MenuItem::separator(),
            export_menu(),
            MenuItem::separator(),
            MenuItem::action("Start MCP Server", StartMcp),
            MenuItem::action("Stop MCP Server", StopMcp),
            MenuItem::separator(),
            MenuItem::action("Reveal in File Manager", RevealDocument),
            MenuItem::separator(),
            MenuItem::action("Quit", Quit),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo", UndoDocument),
            MenuItem::action("Redo", RedoDocument),
            MenuItem::separator(),
            MenuItem::action("Cut", EditorCut),
            MenuItem::action("Copy", EditorCopy),
            MenuItem::action("Paste", EditorPaste),
            MenuItem::action("Select All", EditorSelectAll),
            MenuItem::separator(),
            MenuItem::action("Find in Document", EditorFind),
            MenuItem::action("Go to Line...", GoToLine),
            MenuItem::action("Search in Folder", ShowSearch),
            MenuItem::separator(),
            MenuItem::action("Copy Markdown", CopyMarkdown),
            MenuItem::action("Copy HTML", CopyHtml),
        ]),
        Menu::new("View").items([
            MenuItem::action("Source", ViewSource),
            MenuItem::action("Split", ViewSplit),
            MenuItem::action("Preview", ViewPreview),
            MenuItem::separator(),
            MenuItem::action("Files", ShowFiles),
            MenuItem::action("Outline", ShowOutline),
            MenuItem::action("Search", ShowSearch),
            MenuItem::action("History", ShowHistory),
            MenuItem::separator(),
            MenuItem::action("Appearance...", Appearance),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Toggle Toolbar", ToggleToolbar),
            MenuItem::action("Customize Toolbar...", CustomizeToolbar),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Reset Zoom", ZoomReset),
            MenuItem::separator(),
            MenuItem::action("Next Window", NextWindow),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Insert").items([
            MenuItem::action("Link...", FormatLink),
            MenuItem::action("Table...", InsertTable),
            MenuItem::action("Code Block", InsertCodeBlock),
            MenuItem::action("Math Block", InsertMath),
            MenuItem::action("Image File...", ImageFile),
            MenuItem::action("Image Link...", ImageLink),
            MenuItem::action("Image Asset Location...", ImageLocation),
            MenuItem::action("Mermaid Diagram", InsertMermaid),
            MenuItem::action("Footnote...", InsertFootnote),
            MenuItem::action("Horizontal Rule", InsertRule),
        ]),
        Menu::new("Format").items([
            MenuItem::action("Bold", FormatBold),
            MenuItem::action("Italic", FormatItalic),
            MenuItem::action("Strikethrough", FormatStrike),
            MenuItem::action("Inline Code", FormatCode),
            MenuItem::action("Highlight", ToggleHighlight),
            MenuItem::separator(),
            MenuItem::action("Heading", FormatHeading),
            MenuItem::action("Heading Level...", ChooseHeading),
            MenuItem::action("Bullet List", FormatList),
            MenuItem::action("Ordered List", OrderedList),
            MenuItem::action("Task List", FormatTask),
            MenuItem::action("Quote", FormatQuote),
            table::TableEdit::menu(),
        ]),
        Menu::new("Tools").items([
            MenuItem::action("Spelling Language...", ChooseLanguage),
            MenuItem::action("Command Palette...", CommandPalette),
            MenuItem::action("Go to File...", FilePalette),
            MenuItem::action("Go to Heading...", HeadingPalette),
            MenuItem::action("Check Spelling", ToggleSpelling),
            export_menu(),
        ]),
        Menu::new("Help").items([
            MenuItem::action("Keyboard Shortcuts", HelpShortcuts),
            MenuItem::action("Welcome", HelpWelcome),
            MenuItem::action("Markdown Guide", HelpMarkdown),
            MenuItem::action("About CalmMD", About),
        ]),
    ]
}

fn export_menu() -> MenuItem {
    MenuItem::submenu(Menu::new("Export").items([
        MenuItem::action("PDF...", ExportPdf),
        MenuItem::action("HTML...", ExportHtml),
        MenuItem::action("Copy HTML", CopyHtml),
    ]))
}

struct WindowEntry {
    handle: AnyWindowHandle,
    view: Entity<AppView>,
    label: String,
}

#[derive(Clone)]
struct WindowList(Arc<Mutex<Vec<WindowEntry>>>, Arc<AtomicUsize>);

#[allow(dead_code)]
/// Registers actions/key bindings and opens the first shell window.
pub fn init(
    cx: &mut App,
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    platform: Arc<dyn Platform>,
) {
    let windows = WindowList(Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicUsize::new(0)));
    register_actions(cx, services.clone(), events.clone(), platform.clone(), windows.clone());
    open_shell_window(cx, services, events, platform, windows);
}

fn register_actions(
    cx: &mut App,
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    platform: Arc<dyn Platform>,
    windows: WindowList,
) {
    install_app_menus(cx);
    parity::register(cx, &windows);
    table::register(cx, &windows);

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
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-w", CloseDocument, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-w", CloseDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-f", ShowSearch, Some("Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-f", ShowSearch, Some("Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", FormatBold, Some("Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", FormatBold, Some("Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-i", FormatItalic, Some("Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-i", FormatItalic, Some("Input")),
    ]);

    let closed_windows = windows.clone();
    let closed_services = services.clone();
    cx.on_window_closed(move |_, id| {
        let mut entries = closed_windows.0.lock().unwrap();
        if let Some(index) = entries.iter().position(|entry| entry.handle.window_id() == id) {
            let entry = entries.remove(index);
            closed_services.windows.remove(&entry.label);
            closed_services.claims.release(&entry.label);
            closed_services.autosave.dispose_clean_owner(&entry.label);
            let _ = closed_services.workspace.remove_window(&entry.label);
        }
    }).detach();
    let action_open_windows = windows.clone();
    cx.on_action(move |_: &OpenFolder, cx| {
        if let Some(entry) = active_entry(&action_open_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| view.open_folder(cx));
        }
    });

    let file_windows = windows.clone();
    let file_platform = platform.clone();
    cx.on_action(move |_: &OpenFile, cx| {
        if let Some(entry) = active_entry(&file_windows, cx) {
            let state = entry.view.read(cx).state.clone();
            let platform = file_platform.clone();
            cx.spawn(async move |cx| {
                let Some(path) = platform.open_file() else { return };
                let _ = state.update(cx, |state, cx| state.open_dropped_paths(vec![Value::String(path.to_string_lossy().into_owned())], cx));
            }).detach();
        }
    });

    let close_tabs = windows.clone();
    cx.on_action(move |_: &CloseDocument, cx| {
        if let Some(entry) = active_entry(&close_tabs, cx) {
            let state = entry.view.read(cx).state.clone();
            if let Some(path) = state.read(cx).active().map(str::to_owned) {
                state.update(cx, |state, cx| state.close_tab(&path, cx));
            }
        }
    });

    let html_windows = windows.clone();
    cx.on_action(move |_: &ExportHtml, cx| {
        let Some(entry) = active_entry(&html_windows, cx) else { return };
        after_window_action(cx, entry.handle, move |window, cx| parity::export_options(entry.view, window, cx, false));
    });
    let pdf_windows = windows.clone();
    cx.on_action(move |_: &ExportPdf, cx| {
        let Some(entry) = active_entry(&pdf_windows, cx) else { return };
        after_window_action(cx, entry.handle, move |window, cx| parity::export_options(entry.view, window, cx, true));
    });

    let mcp_start_windows = windows.clone();
    let mcp_start_services = services.clone();
    cx.on_action(move |_: &StartMcp, cx| {
        let Some(entry) = active_entry(&mcp_start_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        let manager = mcp_start_services.mcp.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        mcp_runtime().spawn(async move { let _ = tx.send(manager.start().await); });
        cx.spawn(async move |cx| {
            let result = rx.await.unwrap_or_else(|_| Err("MCP server task stopped".into()));
            let _ = cx.update(|cx| match result {
                Ok(status) => {
                    if let (Some(url), Some(token)) = (status.url, status.token) {
                        cx.write_to_clipboard(ClipboardItem::new_string(format!("{url} · token: {token}")));
                    }
                    state.update(cx, |state, cx| state.set_message("MCP connection copied".into(), cx));
                }
                Err(error) => state.update(cx, |state, cx| state.set_message(format!("Could not start MCP: {error}"), cx)),
            });
        }).detach();
    });
    let mcp_stop_windows = windows.clone();
    let mcp_stop_services = services.clone();
    cx.on_action(move |_: &StopMcp, cx| {
        let Some(entry) = active_entry(&mcp_stop_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        let manager = mcp_stop_services.mcp.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        mcp_runtime().spawn(async move { manager.stop().await; let _ = tx.send(()); });
        cx.spawn(async move |cx| {
            let _ = rx.await;
            let _ = state.update(cx, |state, cx| state.set_message("MCP server stopped".into(), cx));
        }).detach();
    });

    let copy_html_windows = windows.clone();
    cx.on_action(move |_: &CopyHtml, cx| {
        let Some(entry) = active_entry(&copy_html_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        let Some(path) = state.read(cx).active().map(str::to_owned) else {
            state.update(cx, |state, cx| state.set_message("Open a document before exporting".into(), cx));
            return;
        };
        let text = state.read(cx).text().to_owned();
        cx.spawn(async move |cx| {
            let result = cx.background_executor().spawn(async move {
                let prefs = calmmd_core::core::export::prefs::resolve_prefs(None);
                calmmd_core::core::export::body::build_html(&text, &path, &prefs, &Value::Null, false)
            }).await;
            let _ = cx.update(|cx| match result {
                Ok(html) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(html));
                    state.update(cx, |state, cx| state.set_message("HTML copied".into(), cx));
                }
                Err(error) => state.update(cx, |state, cx| state.set_message(format!("Export failed: {error}"), cx)),
            });
        }).detach();
    });

    let save_as_windows = windows.clone();
    let save_as_platform = platform.clone();
    cx.on_action(move |_: &SaveAs, cx| {
        let Some(entry) = active_entry(&save_as_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        let Some(name) = state.read(cx).active().map(|path| Path::new(path).file_name().unwrap_or_default().to_string_lossy().into_owned()) else {
            state.update(cx, |state, cx| state.set_message("No document is open".into(), cx)); return;
        };
        let platform = save_as_platform.clone();
        cx.spawn(async move |cx| {
            let Some(path) = platform.save_file(&name) else { return };
            let _ = state.update(cx, |state, cx| state.save_as(path, cx));
        }).detach();
    });

    let save_windows = windows.clone();
    cx.on_action(move |_: &SaveDocument, cx| {
        if let Some(entry) = active_entry(&save_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.save_active(cx));
            });
        }
    });

    let version_windows = windows.clone();
    cx.on_action(move |_: &SaveVersion, cx| {
        let Some(entry) = active_entry(&version_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        let Some(path) = state.read(cx).active().map(str::to_owned) else { return };
        after_window_action(cx, entry.handle, move |window, cx| {
            crate::menu_dialogs::text_prompt(window, cx, "Save Version", "Version name (optional)", "", move |name, _, cx| {
                let document = state.read(cx);
                if document.active() != Some(path.as_str()) {
                    return Err("The active document changed.".into());
                }
                document.services().history.save(&path, document.text(), (!name.is_empty()).then_some(name))?;
                state.update(cx, |state, cx| state.set_message("Version saved".into(), cx));
                Ok(())
            });
        });
    });
    let reveal_windows = windows.clone();
    let reveal_platform = platform.clone();
    cx.on_action(move |_: &RevealDocument, cx| {
        if let Some(entry) = active_entry(&reveal_windows, cx) {
            let state = entry.view.read(cx).state.clone();
            let result = state.read(cx).active().ok_or_else(|| "No document is open".to_owned())
                .and_then(|path| reveal_platform.reveal(Path::new(path)));
            if let Err(error) = result { state.update(cx, |state, cx| state.set_message(error, cx)); }
        }
    });

    let new_doc_windows = windows.clone();
    cx.on_action(move |_: &NewDocument, cx| {
        if let Some(entry) = active_entry(&new_doc_windows, cx) {
            let _ = entry.view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| state.create_untitled(cx));
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
        after_window_action(cx, next, |window, _| window.activate_window());
    });

    let close_windows = windows.clone();
    cx.on_action(move |_: &CloseWindow, cx| {
        if let Some(entry) = active_entry(&close_windows, cx) {
            let can_close = entry.view.update(cx, |view, cx| view.state.update(cx, |state, cx| state.flush_before_close(cx)));
            if can_close {
                after_window_action(cx, entry.handle, |window, _| window.remove_window());
            }
        }
    });
    macro_rules! editor_action {
        ($action:ty, $native:expr) => {{
            let windows = windows.clone();
            cx.on_action(move |_: &$action, cx| {
                if let Some(entry) = active_entry(&windows, cx) {
                    after_window_action(cx, entry.handle, move |window, cx| {
                        dispatch_editor_action(&entry.view, Box::new($native), window, cx);
                    });
                }
            });
        }};
    }
    editor_action!(UndoDocument, gpui_kit::component::input::Undo);
    editor_action!(RedoDocument, gpui_kit::component::input::Redo);
    editor_action!(EditorCut, gpui_kit::component::input::Cut);
    editor_action!(EditorCopy, gpui_kit::component::input::Copy);
    editor_action!(EditorPaste, gpui_kit::component::input::Paste);
    editor_action!(EditorSelectAll, gpui_kit::component::input::SelectAll);
    editor_action!(EditorFind, gpui_kit::component::input::Search);

    macro_rules! format_action {
        ($action:ty, $before:literal, $after:literal) => {{
            let windows = windows.clone();
            cx.on_action(move |_: &$action, cx| {
                if let Some(entry) = active_entry(&windows, cx) {
                    let input = entry.view.read(cx).editor.read(cx).editor.input.clone();
                    let state = entry.view.read(cx).state.clone();
                    if state.read(cx).active().is_none() { return; }
                    after_window_action(cx, entry.handle, move |window, cx| {
                        let text = input.update(cx, |editor, cx| {
                            let selected = editor.selected_value();
                            editor.replace(format!("{}{}{}", $before, selected, $after), window, cx);
                            editor.value().to_string()
                        });
                        state.update(cx, |state, cx| state.editor_changed(text, cx));
                        input.read(cx).focus_handle(cx).focus(window, cx);
                    });
                }
            });
        }};
    }
    register_inline_toggle::<FormatBold>(cx, &windows, "**");
    register_inline_toggle::<FormatItalic>(cx, &windows, "*");
    register_inline_toggle::<FormatStrike>(cx, &windows, "~~");
    register_inline_toggle::<ToggleHighlight>(cx, &windows, "==");
    register_inline_toggle::<FormatCode>(cx, &windows, "`");
    register_insert_dialog::<FormatLink>(cx, &windows, "Insert Link", "Link destination", "", |url, selected| {
        if url.is_empty() || url.chars().any(char::is_control) {
            return Err("Enter a link destination.".into());
        }
        let destination = url.replace('<', "%3C").replace('>', "%3E");
        let label = if selected.is_empty() { "Link" } else { selected };
        Ok(format!("[{label}](<{destination}>)"))
    });
    register_insert_dialog::<InsertTable>(cx, &windows, "Insert Table", "Columns x rows", "2 x 2", markdown_table);
    register_line_format::<FormatHeading>(cx, &windows, "## ");
    register_line_format::<FormatList>(cx, &windows, "- ");
    register_line_format::<OrderedList>(cx, &windows, "1. ");
    register_line_format::<FormatTask>(cx, &windows, "- [ ] ");
    register_line_format::<FormatQuote>(cx, &windows, "> ");
    format_action!(InsertCodeBlock, "```\n", "\n```");
    format_action!(InsertMermaid, "\n\x60\x60\x60mermaid\nflowchart TD\n    A --> B\n\x60\x60\x60\n", "");
    format_action!(InsertMath, "$$\n", "\n$$");
    format_action!(InsertRule, "\n---\n", "");

    macro_rules! sidebar_action {
        ($action:ty, $tab:expr) => {{
            let windows = windows.clone();
            cx.on_action(move |_: &$action, cx| {
                if let Some(entry) = active_entry(&windows, cx) {
                    let sidebar = entry.view.update(cx, |view, cx| {
                        view.sidebar_open = true;
                        cx.notify();
                        view.sidebar.clone()
                    });
                    sidebar.update(cx, |sidebar, cx| { sidebar.tab = $tab; cx.notify(); });
                    if $tab == SidebarTab::Search {
                        let focus = sidebar.read(cx).search.read(cx).focus_handle(cx);
                        after_window_action(cx, entry.handle, move |window, cx| focus.focus(window, cx));
                    }
                }
            });
        }};
    }
    sidebar_action!(ShowFiles, SidebarTab::Files);
    sidebar_action!(ShowOutline, SidebarTab::Outline);
    sidebar_action!(ShowSearch, SidebarTab::Search);
    sidebar_action!(ShowHistory, SidebarTab::History);

    macro_rules! view_action {
        ($action:ty, $mode:expr) => {{
            let windows = windows.clone();
            cx.on_action(move |_: &$action, cx| {
                if let Some(entry) = active_entry(&windows, cx) {
                    entry.view.update(cx, |view, cx| { view.view_mode = $mode; cx.notify(); });
                }
            });
        }};
    }
    view_action!(ViewSource, ViewMode::Source);
    view_action!(ViewSplit, ViewMode::Split);
    view_action!(ViewPreview, ViewMode::Preview);

    let language_windows = windows.clone();
    cx.on_action(move |_: &ChooseLanguage, cx| {
        let Some(entry) = active_entry(&language_windows, cx) else { return };
        let state = entry.view.read(cx).state.clone();
        after_window_action(cx, entry.handle, move |window, cx| open_language_dialog(state, window, cx));
    });
    cx.on_action(|_: &About, _| {
        rfd::MessageDialog::new().set_title("About CalmMD")
            .set_description("CalmMD — a local Markdown editor\n\nYour documents stay on your computer.")
            .set_buttons(rfd::MessageButtons::Ok).show();
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
    let copy_windows = windows.clone();
    let copy_platform = platform.clone();
    cx.on_action(move |_: &CopyMarkdown, cx| {
        if let Some(entry) = active_entry(&copy_windows, cx) {
            let text = entry.view.read(cx).state.read(cx).text().to_owned();
            copy_platform.copy(&text, cx);
        }
    });
    cx.on_action(|_: &ToggleTheme, cx| theme::toggle(cx));
    cx.bind_keys([
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", UndoDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", RedoDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-z", RedoDocument, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", UndoDocument, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", RedoDocument, None),
        KeyBinding::new("ctrl-alt-z", UndoDocument, None),
        KeyBinding::new("ctrl-alt-shift-z", RedoDocument, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-z", gpui_kit::component::input::Redo, Some("Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-t", ToggleTheme, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-t", ToggleTheme, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-c", CopyMarkdown, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-c", CopyMarkdown, None),
    ]);
    let quit_windows = windows.clone();
    cx.on_action(move |_: &Quit, cx| {
        let views: Vec<_> = quit_windows.0.lock().unwrap().iter().map(|entry| entry.view.clone()).collect();
        if views.into_iter().all(|view| view.update(cx, |view, cx| view.state.update(cx, |state, cx| state.flush_before_close(cx)))) {
            cx.quit();
        }
    });

    // Also flush when the OS initiates application shutdown rather than the menu action.
    let quit_services = services.clone();
    cx.on_app_quit(move |_| {
        for window in quit_services.windows.windows() {
            if let Err(error) = quit_services.autosave.flush_owner(&window.label) {
                eprintln!("Could not save before quitting: {error}");
            }
        }
        async {}
    }).detach();

}

fn open_shell_window(
    cx: &mut App,
    services: Arc<Services>,
    events: Entity<CoreEvents>,
    platform: Arc<dyn Platform>,
    windows: WindowList,
) {
    // The core registry starts with "main", so count actual shell windows here. The first window
    // must use "main" too, or the restored session and MCP's default target are not visible.
    let label = window_label(windows.1.fetch_add(1, Ordering::Relaxed));
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
    windows.0.lock().unwrap().push(WindowEntry { handle, view, label });
}

/// Root view of one GPUI window.
pub struct AppView {
    state: Entity<WorkspaceState>,
    #[cfg(not(target_os = "macos"))]
    menu: Entity<AppMenuBar>,
    sidebar: Entity<SidebarPanel>,
    editor: Entity<EditorPanel>,
    preview: Entity<PreviewPanel>,
    status: Entity<StatusPanel>,
    view_mode: ViewMode,
    sidebar_open: bool,
    toolbar_visible: bool,
    toolbar_items: Vec<String>,
    images_in_root: bool,
    zoom: f32,
    last_title: String,
    platform: Arc<dyn Platform>,
    #[allow(dead_code)]
    window_label: String,
    #[allow(dead_code)]
    services: Arc<Services>,
    #[allow(dead_code)]
    events: Entity<CoreEvents>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode { Source, Split, Preview }

impl AppView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        services: Arc<Services>,
        events: Entity<CoreEvents>,
        platform: Arc<dyn Platform>,
        window_label: String,
    ) -> Self {
        let state = cx.new(|cx| WorkspaceState::new(services.clone(), events.clone(), window_label.clone(), cx));
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let close_state = state.clone();
        window.on_window_should_close(cx, move |_, cx| {
            close_state.update(cx, |state, cx| state.flush_before_close(cx))
        });
        #[cfg(not(target_os = "macos"))]
        let menu = AppMenuBar::new(cx);
        let sidebar = cx.new(|cx| { cx.observe(&state, |_, _, cx| cx.notify()).detach(); SidebarPanel::new(state.clone(), platform.clone(), window, cx) });
        let editor = cx.new(|cx| { cx.observe(&state, |_, _, cx| cx.notify()).detach(); EditorPanel::new(state.clone(), window, cx) });
        let preview = cx.new(|cx| { cx.observe(&state, |_, _, cx| cx.notify()).detach(); PreviewPanel::new(state.clone(), platform.clone(), cx) });
        let status = cx.new(|cx| { cx.observe(&state, |_, _, cx| cx.notify()).detach(); StatusPanel::new(state.clone(), cx) });

        let toolbar_items = toolbar::load(state.read(cx));
        let images_in_root = calmmd_core::kv::read(state.read(cx).context(), "gpui/image-location")
            .ok().flatten().as_deref() == Some("root");
        Self {
            state,
            #[cfg(not(target_os = "macos"))]
            menu,
            sidebar,
            editor,
            preview,
            status,
            view_mode: ViewMode::Split,
            sidebar_open: true,
            toolbar_visible: true,
            toolbar_items,
            images_in_root,
            zoom: 1.0,
            last_title: String::new(),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let theme = Theme::global(cx);
        let background = theme.background;
        let title = self.state.read(cx).title();
        let title = format!("{title} — CalmMD");
        if self.last_title != title {
            window.set_window_title(&title);
            self.last_title = title;
        }
        let mut root = div().v_flex().size_full();
        #[cfg(not(target_os = "macos"))]
        {
            // AppMenuBar fills its parent. Without a fixed-height wrapper it consumes the whole
            // window and leaves the dock zero pixels tall.
            root = root.child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .bg(background)
                    .child(self.menu.clone()),
            );
        }
        let mut tabs = div().h_flex().items_center().h(px(38.)).flex_shrink_0().border_b_1().border_color(theme.border).px(px(8.));
        let active = self.state.read(cx).active().map(str::to_owned);
        for path in self.state.read(cx).tabs() {
            let path = path.clone();
            let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
            let selected = active.as_deref() == Some(&path);
            let open_state = self.state.clone();
            let close_state = self.state.clone();
            let close_path = path.clone();
            tabs = tabs.child(div().h_flex().items_center().when(selected, |el| el.border_b_2().border_color(theme.accent))
                .child(Button::new(ElementId::Name(format!("tab:{path}").into())).label(name).ghost().compact()
                    .on_click(move |_, _, cx| open_state.update(cx, |state, cx| state.open_document(&path, cx))))
                .child(Button::new(ElementId::Name(format!("close-tab:{close_path}").into())).label("×").ghost().compact()
                    .on_click(move |_, _, cx| close_state.update(cx, |state, cx| state.close_tab(&close_path, cx)))));
        }
        tabs = tabs.child(Button::new("add-tab").label("+").ghost().compact()
            .on_click(|_, window, cx| window.dispatch_action(Box::new(NewDocument), cx)));
        let panes = match self.view_mode {
            ViewMode::Split => h_resizable("document-panes")
                .child(resizable_panel().size_range(px(180.)..px(4000.)).child(self.editor.clone()))
                .child(resizable_panel().size_range(px(180.)..px(4000.)).child(self.preview.clone())).into_any_element(),
            ViewMode::Source => div().size_full().child(self.editor.clone()).into_any_element(),
            ViewMode::Preview => div().size_full().child(self.preview.clone()).into_any_element(),
        };
        let main = div().v_flex().size_full().child(tabs)
            .when(self.toolbar_visible && self.view_mode != ViewMode::Preview, |main| main.child(toolbar::render(&self.toolbar_items, theme.border)))
            .child(div().flex_1().min_h(px(0.)).child(panes));
        let body = if self.sidebar_open {
            h_resizable("workspace-columns")
                .child(resizable_panel().size(px(248.)).size_range(px(180.)..px(480.)).fixed().child(self.sidebar.clone()))
                .child(resizable_panel().child(main)).into_any_element()
        } else {
            main.into_any_element()
        };
        root.can_drop(|value, _, _| value.downcast_ref::<ExternalPaths>().is_some())
            .on_drop::<ExternalPaths>(move |paths, _, cx| {
                let values: Vec<Value> = paths
                    .paths()
                    .iter()
                    .map(|path| Value::String(path.to_string_lossy().into_owned()))
                    .collect();
                let _ = state.update(cx, |state, cx| state.open_dropped_paths(values, cx));
            })
            .on_mouse_down(MouseButton::Right, |event, window, cx| {
                NativeMenu::new()
                    .menu("New Document", Box::new(NewDocument))
                    .menu("Open Folder…", Box::new(OpenFolder))
                    .separator()
                    .menu("Undo", Box::new(UndoDocument))
                    .menu("Redo", Box::new(RedoDocument))
                    .separator()
                    .menu("Toggle Theme", Box::new(ToggleTheme))
                    .show(event.position, window, cx);
            })
            .child(div().flex_1().min_h(px(0.)).child(body))
            .child(div().h(px(28.)).flex_shrink_0().border_t_1().border_color(theme.border).child(self.status.clone()))
    }
}

fn window_label(open_windows: usize) -> String {
    if open_windows == 0 {
        "main".to_owned()
    } else {
        format!("window-{}", open_windows + 1)
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
            label: entry.label.clone(),
        })
}

fn open_language_dialog(state: Entity<WorkspaceState>, window: &mut Window, cx: &mut App) {
    let dictionaries = match calmmd_core::core::spell::dictionaries(state.read(cx).context()) {
        Ok(dictionaries) => dictionaries,
        Err(error) => {
            state.update(cx, |state, cx| state.set_message(error, cx));
            return;
        }
    };
    window.open_dialog(cx, move |dialog, _, cx| {
        let selected = state.read(cx).spell_language();
        let mut choices = div().id("spelling-languages").v_flex().gap_1().max_h(px(320.)).overflow_y_scroll();
        for dictionary in dictionaries.iter().filter(|dictionary| dictionary.installed) {
            let Some(language) = dictionary.languages.first().cloned() else { continue };
            let state = state.clone();
            let is_selected = selected == Some(&language);
            let label = if is_selected { format!("{} (selected)", dictionary.label) } else { dictionary.label.clone() };
            choices = choices.child(Button::new(ElementId::Name(dictionary.id.clone().into())).label(label)
                .on_click(move |_, window, cx| {
                    state.update(cx, |state, cx| state.set_spell_language(Some(language.clone()), cx));
                    window.close_dialog(cx);
                }));
        }
        let state = state.clone();
        dialog.title("Spelling Language").child(choices)
            .footer(Button::new("disable-spelling").label("Turn off spell check").on_click(move |_, window, cx| {
                state.update(cx, |state, cx| state.set_spell_language(None, cx));
                window.close_dialog(cx);
            }))
    });
}

fn export_document(view: &Entity<AppView>, pdf: bool, options: Value, cover: Value, cx: &mut App) {
    let state = view.read(cx).state.clone();
    let document = state.read(cx);
    let Some(path) = document.active().map(str::to_owned) else {
        state.update(cx, |state, cx| state.set_message("Open a document before exporting".into(), cx));
        return;
    };
    let text = document.text().to_owned();
    let extension = if pdf { "pdf" } else { "html" };
    let name = Path::new(&path).with_extension(extension).file_name().unwrap_or_default().to_string_lossy().into_owned();
    let platform = view.read(cx).platform.clone();
    cx.spawn(async move |cx| {
        let Some(output) = platform.save_file(&name) else { return };
        let out = output.to_string_lossy().into_owned();
        let displayed_path = out.clone();
        let result = cx.background_executor().spawn(async move {
            let source = Path::new(&path);
            if source == output || std::fs::canonicalize(&output).ok().zip(std::fs::canonicalize(source).ok())
                .is_some_and(|(output, source)| output == source) {
                return Err("Choose an export destination different from the source document.".into());
            }
            use calmmd_core::core::export::{body, prefs};
            let prefs = prefs::resolve_prefs(Some(&options));
            let fallback = Path::new(&path).file_stem().unwrap_or_default().to_string_lossy();
            let cover = prefs::resolve_cover(&text, &fallback, (!cover.is_null()).then_some(&cover), &prefs);
            if pdf {
                body::export_pdf(&path, &out, &text, &prefs, &cover)
            } else {
                body::export_html(&path, &out, &text, &prefs, &cover)
            }
        }).await;
        let message = match result {
            Ok(_) => format!("Exported {displayed_path}"),
            Err(error) => format!("Export failed: {error}"),
        };
        let _ = state.update(cx, |state, cx| state.set_message(message, cx));
    }).detach();
}

fn register_inline_toggle<A: Action>(cx: &mut App, windows: &WindowList, marker: &'static str) {
    let windows = windows.clone();
    cx.on_action(move |_: &A, cx| {
        let Some(entry) = active_entry(&windows, cx) else { return };
        if entry.view.read(cx).state.read(cx).active().is_none() { return; }
        let input = entry.view.read(cx).editor.read(cx).editor.input.clone();
        after_window_action(cx, entry.handle, move |window, cx| {
            input.update(cx, |editor, cx| {
                let value = editor.value();
                let range = editor.selected_range();
                let selected = &value[range.clone()];
                if let Some(inner) = selected.strip_prefix(marker).and_then(|part| part.strip_suffix(marker)) {
                    editor.replace(inner, window, cx);
                } else if value[..range.start].ends_with(marker) && value[range.end..].starts_with(marker) {
                    editor.set_selected_range(range.start - marker.len()..range.end + marker.len(), cx);
                    editor.replace(selected, window, cx);
                } else if !selected.is_empty() {
                    editor.replace(format!("{marker}{selected}{marker}"), window, cx);
                    editor.set_selected_range(range.start + marker.len()..range.end + marker.len(), cx);
                }
            });
        });
    });
}

fn register_line_format<A: Action>(cx: &mut App, windows: &WindowList, prefix: &'static str) {
    let windows = windows.clone();
    cx.on_action(move |_: &A, cx| {
        let Some(entry) = active_entry(&windows, cx) else { return };
        let view = entry.view.read(cx);
        let state = view.state.clone();
        if state.read(cx).active().is_none() { return; }
        let input = view.editor.read(cx).editor.input.clone();
        after_window_action(cx, entry.handle, move |window, cx| {
            input.update(cx, |editor, cx| {
                let source = editor.value();
                let selection = editor.selected_range();
                let start = source[..selection.start].rfind('\n').map_or(0, |offset| offset + 1);
                // A selection ending at the start of a line excludes that line.
                let end = if selection.end > selection.start && source[..selection.end].ends_with('\n') {
                    selection.end - 1
                } else {
                    source[selection.end..].find('\n').map_or(source.len(), |offset| selection.end + offset)
                };
                let replacement = source[start..end].split('\n').map(|line| {
                    if let Some(rest) = line.strip_prefix(prefix) { rest.to_owned() }
                    else { format!("{prefix}{line}") }
                }).collect::<Vec<_>>().join("\n");
                editor.set_selected_range(start..end, cx);
                editor.replace(replacement, window, cx);
            });
            input.read(cx).focus_handle(cx).focus(window, cx);
        });
    });
}

fn register_insert_dialog<A: Action>(
    cx: &mut App,
    windows: &WindowList,
    title: &'static str,
    label: &'static str,
    initial: &'static str,
    build: fn(&str, &str) -> Result<String, String>,
) {
    let windows = windows.clone();
    cx.on_action(move |_: &A, cx| {
        let Some(entry) = active_entry(&windows, cx) else { return };
        let view = entry.view.read(cx);
        let state = view.state.clone();
        let Some(path) = state.read(cx).active().map(str::to_owned) else {
            state.update(cx, |state, cx| state.set_message("Open a document first.".into(), cx));
            return;
        };
        let input = view.editor.read(cx).editor.input.clone();
        let original = input.read(cx).value();
        let selection = input.read(cx).selected_range();
        after_window_action(cx, entry.handle, move |window, cx| {
            crate::menu_dialogs::text_prompt(window, cx, title, label, initial, move |value, window, cx| {
                if state.read(cx).active() != Some(path.as_str()) || input.read(cx).value() != original || state.read(cx).text() != original.as_ref() {
                    return Err("The document changed. Reopen this dialog to try again.".into());
                }
                let replacement = build(value, original.get(selection.clone()).unwrap_or_default())?;
                input.update(cx, |editor, cx| {
                    editor.set_selected_range(selection.clone(), cx);
                    editor.replace(replacement, window, cx);
                });
                Ok(())
            });
        });
    });
}

fn markdown_table(dimensions: &str, selection: &str) -> Result<String, String> {
    let (columns, rows) = dimensions.split_once(['x', 'X', '\u{00d7}']).ok_or("Enter columns x rows, for example 3 x 2.")?;
    let parse = |value: &str| value.trim().parse::<usize>().ok().filter(|size| (1..=30).contains(size));
    let (Some(columns), Some(rows)) = (parse(columns), parse(rows)) else {
        return Err("Use a number from 1 to 30 for each dimension.".into());
    };
    let row = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let mut table = vec![
        row((1..=columns).map(|i| format!("Column {i}")).collect()),
        row(vec!["---".into(); columns]),
    ];
    let mut first_cell = vec![String::new(); columns];
    first_cell[0] = selection.lines().collect::<Vec<_>>().join(" ").replace('|', r"\|");
    table.push(row(first_cell));
    table.extend((1..rows).map(|_| row(vec![String::new(); columns])));
    Ok(format!("\n\n{}\n\n", table.join("\n")))
}

/// GPUI temporarily takes the window out of the application during a click
/// or shortcut dispatch. Reopening its handle in the callback would fail.
fn after_window_action(cx: &mut App, handle: AnyWindowHandle, run: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        if let Err(error) = handle.update(cx, |_, window, cx| run(window, cx)) {
            eprintln!("Could not dispatch CalmMD window action: {error}");
        }
    });
}

fn dispatch_editor_action(view: &Entity<AppView>, action: Box<dyn Action>, window: &mut Window, cx: &mut App) {
    if view.read(cx).state.read(cx).active().is_none() {
        return;
    }
    let focus = view.read(cx).editor.read(cx).editor.input.read(cx).focus_handle(cx);
    if view.read(cx).view_mode == ViewMode::Preview {
        view.update(cx, |view, cx| {
            view.view_mode = ViewMode::Split;
            cx.notify();
        });
        window.on_next_frame(move |window, cx| {
            focus.focus(window, cx);
            window.dispatch_action(action, cx);
        });
    } else {
        // Target the editor's node directly. The menu or toolbar can own focus.
        focus.focus(window, cx);
        focus.dispatch_action(action.as_ref(), window, cx);
    }
}

fn panel_focus(cx: &App) -> FocusHandle {
    cx.focus_handle()
}

fn mcp_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("MCP runtime"))
}

struct SidebarPanel {
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
    search: Entity<InputState>,
    focus: FocusHandle,
    tab: SidebarTab,
    expanded: HashSet<String>,
    _search_subscription: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarTab { Files, Outline, Search, History }

fn file_tree_rows(state: &WorkspaceState, expanded: &HashSet<String>) -> Vec<(String, String, usize, bool)> {
    fn walk(state: &WorkspaceState, dir: &str, depth: usize, expanded: &HashSet<String>, seen: &mut HashSet<std::path::PathBuf>, rows: &mut Vec<(String, String, usize, bool)>) {
        if depth > 32 || rows.len() >= 5000 { return; }
        let Ok(real) = std::fs::canonicalize(dir) else { return };
        if !seen.insert(real) { return; }
        let Ok(entries) = calmmd_core::tree::list(Path::new(dir), false) else { return };
        for entry in entries {
            if state.services().workspace.resolve(state.window_label(), &entry.path).is_err() { continue; }
            if !entry.is_dir && !calmmd_core::core::paths::is_markdown_path(&entry.name) { continue; }
            let path = entry.path;
            rows.push((path.clone(), entry.name, depth, entry.is_dir));
            if entry.is_dir && expanded.contains(&path) { walk(state, &path, depth + 1, expanded, seen, rows); }
        }
    }
    let mut rows = Vec::new();
    if let Some(root) = state.root() { walk(state, root, 0, expanded, &mut HashSet::new(), &mut rows); }
    rows
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
            tab: SidebarTab::Files,
            expanded: HashSet::new(),
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
"Files"
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
        let theme = Theme::global(cx);
        let root_name = state
            .root()
            .and_then(|path| path.rsplit(['/', '\\']).find(|part| !part.is_empty()))
            .unwrap_or("No folder");
        let active = state.active().unwrap_or("");

        let sidebar_bg = if theme.mode.is_dark() { rgb(0x1c2028) } else { rgb(0xf5f6f8) };
        let mut column = div().id("sidebar-content").v_flex().size_full().overflow_y_scroll().bg(sidebar_bg);
        column = column.child(
            div().px(px(12.0)).pt(px(10.0)).pb(px(8.0)).child(
                Button::new("spaces").label(state.space_name().unwrap_or_else(|| "No Space".into()))
                    .ghost().w_full().on_click(|_, window, cx| window.dispatch_action(Box::new(Spaces), cx)),
            ),
        );
        let mut nav = div().h_flex().items_center().mx(px(10.)).mb(px(8.)).gap(px(1.));
        for (tab, id, title) in [
            (SidebarTab::Files, "side-files", "Files"),
            (SidebarTab::Outline, "side-outline", "Outline"),
            (SidebarTab::Search, "side-search", "Search"),
            (SidebarTab::History, "side-history", "History"),
        ] {
            nav = nav.child(Button::new(id).label(title).ghost().compact()
                .when(self.tab == tab, |button| button.bg(theme.muted))
                .on_click(cx.listener(move |this, _, _, cx| { this.tab = tab; cx.notify(); })));
        }
        column = column.child(nav);
        if self.tab == SidebarTab::Files {
        column = column.child(
            div()
                .h_flex()
                .px(px(12.0))
                .pb(px(2.0))
                .gap(px(6.0))
                .child(Label::new("FAVOURITES")),
        );
        column = column.child(
            div().px(px(12.0)).pb(px(8.0)).child(
                Label::new("Star a file or folder to keep it here.")
                    .text_color(theme.muted_foreground),
            ),
        );
        column = column.child(
            div()
                .h_flex()
                .px(px(12.0))
                .py(px(6.0))
                .gap(px(6.0))
                .child(Label::new(root_name)),
        );

        if state.root().is_none() {
            column = column.child(
                div().px(px(12.0)).child(
                    Button::new("open-folder")
                        .label("Open folder")
                        .compact()
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
                ),
            );
        }

        if !state.recovery_drafts().is_empty() {
            column = column.child(div().px(px(12.0)).py(px(6.0)).child(Label::new("RECOVER DRAFTS")));
            for draft in state.recovery_drafts() {
                let id = draft.id.clone();
                let path = draft.record.path.as_deref().unwrap_or("");
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
                let state = self.state.clone();
                column = column.child(div().px(px(8.0)).child(
                    Button::new(ElementId::Name(format!("recover-draft:{id}").into()))
                        .label(format!("Recover {name}"))
                        .compact()
                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.recover_draft(&id, cx)))
                ));
            }
        }

        for (path, name, depth, is_dir) in file_tree_rows(state, &self.expanded) {
            let selected = path == active;
            let expanded = self.expanded.contains(&path);
            let icon = if is_dir { if expanded { "⌄ ▱" } else { "› ▱" } } else { "▤" };
            let id = format!("tree:{path}");
            column = column.child(div().pl(px(8. + depth as f32 * 14.)).child(
                Button::new(ElementId::Name(id.into())).label(format!("{icon}  {name}"))
                    .ghost().compact().w_full().justify_start().when(selected, |button| button.bg(theme.selection))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if is_dir {
                            if !this.expanded.remove(&path) { this.expanded.insert(path.clone()); }
                            cx.notify();
                        } else {
                            this.state.update(cx, |state, cx| state.open_document(&path, cx));
                        }
                    }))
            ));
        }
        } else if self.tab == SidebarTab::Search {
        column = column.child(div().px(px(12.0)).pb(px(8.0)).child(Input::new(&self.search)));
        for hit in state.search_results() {
            let line = hit.line;
            let path = hit.path.clone();
            let state = self.state.clone();
            column = column.child(
                div().px(px(8.0)).child(
                    Button::new(ElementId::Name(format!("search-hit:{path}:{}", hit.line).into()))
                        .label(format!(
                            "{}:{} {}",
                            hit.path.rsplit(['/', '\\']).next().unwrap_or(&hit.path),
                            hit.line + 1,
                            hit.text.trim()
                        ))
                        .compact()
                        .on_click(move |_, _window, cx| {
                            state.update(cx, |state, cx| state.open_document(&path, cx));
                            if state.read(cx).active() == Some(path.as_str()) {
                                parity::jump_in_window(_window, cx, line);
                            }
                        }),
                ),
            );
        }
        } else if self.tab == SidebarTab::Outline {
            if let Ok(parsed) = calmmd_core::core::markdown::parse_document(state.text()) {
                if parsed.headings.is_empty() {
                    column = column.child(div().px(px(12.)).child(Label::new("No headings").text_color(theme.muted_foreground)));
                }
                for heading in parsed.headings {
                    column = column.child(div().px(px(12. + 10. * (heading.level.saturating_sub(1) as f32)))
                        .child(Button::new(("outline-line", heading.line)).label(heading.text).ghost().compact()
                            .on_click(move |_, window, cx| parity::jump_in_window(window, cx, heading.line))));
                }
            }
        } else {
            let versions = state.active().and_then(|path| state.services().history.list(path).ok()).unwrap_or_default();
            if versions.is_empty() {
                column = column.child(div().px(px(12.)).child(Label::new("No saved versions").text_color(theme.muted_foreground)));
            }
            for version in versions {
                let state = self.state.clone();
                let Some(path) = state.read(cx).active().map(str::to_owned) else { break };
                let caption = version.name.unwrap_or_else(|| "Saved version".into());
                let id = version.id;
                column = column.child(Button::new(ElementId::Name(format!("restore-version:{id}").into()))
                    .label(format!("Restore {caption}")).ghost().compact()
                    .on_click(move |_, window, cx| {
                        let state = state.clone();
                        let path = path.clone();
                        let id = id.clone();
                        window.open_dialog(cx, move |dialog, _, _| {
                            let state = state.clone();
                            let path = path.clone();
                            let id = id.clone();
                            dialog.title("Restore version").child(Label::new("CalmMD will save the current text in history before restoring this version."))
                                .footer(Button::new("restore-version-confirm").label("Restore").on_click(move |_, window, cx| {
                                    let result = state.update(cx, |state, cx| state.restore_version(&path, &id, cx));
                                    if let Err(error) = result {
                                        state.update(cx, |state, cx| state.set_message(error, cx));
                                    }
                                    window.close_dialog(cx);
                                }))
                        });
                    }));
            }
        }

        column = column.child(div().flex_1());
        column.child(
            div()
                .h_flex()
                .px(px(10.0))
                .py(px(8.0))
                .gap(px(4.0))
                .child(
                    Button::new("new-document")
                        .label("New")
                        .compact()
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(NewDocument), cx)),
                )
                .child(
                    Button::new("save-document")
                        .label("Save")
                        .compact()
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(SaveDocument), cx)),
                )
                .child(
                    Button::new("open-folder-footer")
                        .label("Folder")
                        .compact()
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenFolder), cx)),
                ),
        )
    }
}

struct EditorPanel {
    state: Entity<WorkspaceState>,
    editor: MarkdownEditor,
    zoom: f32,
}

impl EditorPanel {
    fn new(state: Entity<WorkspaceState>, window: &mut Window, cx: &mut App) -> Self {
        let text = state.read(cx).text().to_owned();
        let editor = MarkdownEditor::new(window, cx, state.clone(), &text);
        Self { state, editor, zoom: 1.0 }
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
"Editor"
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
            .child(Editor::new(&self.editor.input).text_size(px(18. * self.zoom)).h(gpui_kit::relative(1.0)))
    }
}

struct PreviewPanel {
    state: Entity<WorkspaceState>,
    platform: Arc<dyn Platform>,
    focus: FocusHandle,
    zoom: f32,
}

impl PreviewPanel {
    fn new(state: Entity<WorkspaceState>, platform: Arc<dyn Platform>, cx: &mut App) -> Self {
        Self {
            state,
            platform,
            focus: panel_focus(cx),
            zoom: 1.0,
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
"Preview"
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
        div().size_full().px(px(36.0)).pt(px(38.0)).text_size(px(18. * self.zoom)).font_family("Georgia").child(view)
    }
}

struct StatusPanel {
    state: Entity<WorkspaceState>,
    focus: FocusHandle,
}

impl StatusPanel {
    fn new(
        state: Entity<WorkspaceState>,
        cx: &mut App,
    ) -> Self {
        Self {
            state,
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
        let text = state.text();
        let words = text.split_whitespace().count();
        let chars = text.chars().count();
        let lines = text.lines().count().max(1);
        let save_status = if state.conflict() { "Changed on disk" } else {
            match state.autosave_status() { "dirty" => "Edited", "saving" => "Saving…", "error" => "Couldn’t save, retrying", _ => "Saved" }
        };
        let mut row = div()
            .h_flex()
            .items_center()
            .gap(px(12.0))
            .px(px(10.0))
            .size_full()
            .child(Label::new(save_status))
            .child(Label::new(state.message().to_owned()).text_color(Theme::global(cx).muted_foreground))
            .child(div().flex_1())
            .child(Label::new(format!("{words} words  ·  {chars} characters  ·  {lines} lines")))
            .child(
                Button::new("cycle-spell")
                    .label(state.spell_language().unwrap_or("Spelling off").to_owned())
                    .ghost()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ChooseLanguage), cx)),
            )
            .child(
                Button::new("toggle-theme")
                    .label("◐")
                    .ghost()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Appearance), cx)),
            );
        if state.conflict() {
            row = row
                .child(Button::new("keep-mine").label("Keep mine").on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(KeepMine), cx);
                }))
                .child(Button::new("load-theirs").label(if state.missing_conflict_file() { "Discard draft" } else { "Load theirs" }).on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(LoadTheirs), cx);
                }));
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::app_menus;

    #[test]
    fn first_window_uses_the_core_main_label() {
        assert_eq!(super::window_label(0), "main");
        assert_eq!(super::window_label(1), "window-2");
    }

    #[test]
    fn table_dialog_validates_dimensions_and_preserves_selected_text() {
        assert!(super::markdown_table("0 x 2", "").is_err());
        assert!(super::markdown_table("3 x 300", "").is_err());
        let table = super::markdown_table("3 x 2", "one|two").unwrap();
        let lines = table.trim().lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1], "| --- | --- | --- |");
        assert!(lines[2].contains(r"one\|two"));
    }

    #[test]
    fn app_menus_cover_file_edit_and_view_actions() {
        let menus = app_menus();
        assert_eq!(menus.iter().map(|menu| menu.name.to_string()).collect::<Vec<_>>(), ["File", "Edit", "View", "Insert", "Format", "Tools", "Help"]);
        let edit_labels = menus[1]
            .items
            .iter()
            .filter_map(|item| match item {
                gpui_kit::MenuItem::Action { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(edit_labels.contains(&"Copy Markdown".to_owned()));
    }
}
