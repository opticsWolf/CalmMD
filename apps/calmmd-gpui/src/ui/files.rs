use super::*;
use calmmd_core::core::paths::{basename, dirname, path_key};
use gpui_kit::{assets::IconName, component::Icon};

#[derive(Clone, Copy, PartialEq)]
enum FileOperation {
    Open,
    OpenInTab,
    Bookmark,
    Rename,
    Duplicate,
    Move,
    Reveal,
    CopyPath,
    Trash,
    NewDocument,
    NewFolder,
    CloseRoot,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = calmmd, no_json)]
struct FileAction {
    path: String,
    operation: FileOperation,
}

pub(super) fn register(cx: &mut App, windows: &WindowList) {
    let windows = windows.clone();
    cx.on_action(move |action: &FileAction, cx| {
        let Some(entry) = active_entry(&windows, cx) else {
            return;
        };
        let action = action.clone();
        after_window_action(cx, entry.handle, move |window, cx| {
            execute(action, entry.view, window, cx)
        });
    });
}

fn execute(action: FileAction, view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let active_view = view.read(cx);
    let state = active_view.state.clone();
    let platform = active_view.platform.clone();
    let path = action.path;
    match action.operation {
        FileOperation::Open | FileOperation::OpenInTab => state.update(cx, |state, cx| {
            state.open_favourite(&path, action.operation == FileOperation::OpenInTab, cx)
        }),
        FileOperation::Bookmark => state.update(cx, |state, cx| state.toggle_favourite(&path, cx)),
        FileOperation::CloseRoot => state.update(cx, |state, cx| {
            if let Err(error) = state.close_root(&path, cx) {
                state.set_message(error, cx);
            }
        }),
        FileOperation::Reveal => {
            if let Err(error) = platform.reveal(Path::new(&path)) {
                state.update(cx, |state, cx| state.set_message(error, cx));
            }
        }
        FileOperation::CopyPath => platform.copy(&path, cx),
        FileOperation::Rename
        | FileOperation::Duplicate
        | FileOperation::Move
        | FileOperation::NewDocument
        | FileOperation::NewFolder => {
            let (title, hint, initial) = match action.operation {
                FileOperation::Rename => ("Rename", "Name", basename(&path)),
                FileOperation::Duplicate => (
                    "Duplicate",
                    "Name",
                    format!(
                        "{} copy.md",
                        Path::new(&path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ),
                ),
                FileOperation::Move => ("Move to Folder", "Destination folder", dirname(&path)),
                FileOperation::NewDocument => ("New Document", "Name", "Untitled.md".into()),
                FileOperation::NewFolder => ("New Folder", "Name", "".into()),
                _ => unreachable!(),
            };
            crate::menu_dialogs::text_prompt(
                window,
                cx,
                title,
                hint,
                &initial,
                move |value, _, cx| {
                    let workspace = &state.read(cx).services().workspace;
                    let label = state.read(cx).window_label();
                    let created = match action.operation {
                        FileOperation::Rename => workspace.rename_document(label, &path, value)?,
                        FileOperation::Move => {
                            workspace.move_document(label, &path, value, None)?
                        }
                        FileOperation::Duplicate => {
                            let source = workspace.read_document(label, &path)?;
                            workspace.create_document(
                                label,
                                Some(&dirname(&path)),
                                value,
                                source["text"].as_str(),
                            )?
                        }
                        FileOperation::NewDocument => {
                            workspace.create_document(label, Some(&path), value, None)?
                        }
                        FileOperation::NewFolder => {
                            workspace.create_folder(label, Some(&path), value)?
                        }
                        _ => unreachable!(),
                    };
                    if matches!(
                        action.operation,
                        FileOperation::NewDocument | FileOperation::Duplicate
                    ) {
                        if let Some(path) = created["path"].as_str() {
                            state.update(cx, |state, cx| state.open_document(path, cx));
                        }
                    }
                    Ok(())
                },
            );
        }
        FileOperation::Trash => {
            let name = basename(&path);
            window.open_dialog(cx, move |dialog, _, cx| {
                let (state, path) = (state.clone(), path.clone());
                dialog
                    .title(tr(cx, "Move to Trash"))
                    .child(Label::new(name.clone()))
                    .footer(
                        div()
                            .h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("cancel-trash")
                                    .label(tr(cx, "Cancel"))
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new("confirm-trash")
                                    .label(tr(cx, "Move to Trash"))
                                    .primary()
                                    .on_click(move |_, window, cx| {
                                        state.update(cx, |state, cx| {
                                            if !state.flush_before_close(cx) {
                                                return;
                                            }
                                            if let Err(error) = state
                                                .services()
                                                .workspace
                                                .trash_document(state.window_label(), &path, true)
                                            {
                                                state.set_message(error, cx);
                                            }
                                        });
                                        window.close_dialog(cx);
                                    }),
                            ),
                    )
            });
        }
    }
}

struct DraggedNote {
    path: String,
    window: String,
}

struct NoteDragPreview(String);
impl Render for NoteDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(Theme::global(cx).muted)
            .child(Label::new(self.0.clone()))
    }
}

struct Entry {
    path: String,
    depth: usize,
    kind: EntryKind,
}

enum EntryKind {
    Folder { open: bool, root: bool },
    Document,
    Favourite,
}

impl Entry {
    fn row(
        self,
        state: &Entity<WorkspaceState>,
        favourites: &HashSet<String>,
        cx: &mut Context<SidebarPanel>,
    ) -> AnyElement {
        let path = self.path;
        let name = basename(&path);
        let marked = favourites.contains(&path_key(&path));
        let root = matches!(self.kind, EntryKind::Folder { root: true, .. });
        let directory = matches!(self.kind, EntryKind::Folder { .. }) || Path::new(&path).is_dir();
        let favourite = matches!(self.kind, EntryKind::Favourite);
        let selected = state
            .read(cx)
            .active()
            .is_some_and(|active| path_key(active) == path_key(&path));
        let icon = match self.kind {
            EntryKind::Folder { open: true, .. } => IconName::FolderOpen,
            EntryKind::Folder { open: false, .. } => IconName::FolderClosed,
            EntryKind::Document => IconName::FileText,
            EntryKind::Favourite => IconName::Star,
        };
        let selection = Theme::global(cx).selection;
        let caption = div()
            .h_flex()
            .w_full()
            .min_w(px(0.))
            .gap_2()
            .items_center()
            .child(Icon::new(icon).size(px(15.)))
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(name.clone()),
            );
        let dragged = (!directory && Path::new(&path).is_file()).then(|| DraggedNote {
            path: path.clone(),
            window: state.read(cx).window_label().to_owned(),
        });
        let destination = directory.then(|| path.clone());
        let drop_state = state.clone();
        let open = path.clone();
        let bookmark = path.clone();
        let close = path.clone();
        let menu_path = path.clone();
        let mark_title = tr(
            cx,
            if marked {
                "Remove from Favourites"
            } else {
                "Add to Favourites"
            },
        );
        let scope = if favourite {
            "favourite"
        } else if root {
            "root"
        } else {
            "tree"
        };
        div()
            .id(SharedString::from(format!("{scope}:{path}")))
            .h_flex()
            .items_center()
            .pl(px(6. + self.depth as f32 * 14.))
            .pr_1()
            .min_w(px(0.))
            .when_some(dragged, |row, note| {
                row.on_drag(note, |note, _, _, cx| {
                    cx.new(|_| NoteDragPreview(basename(&note.path)))
                })
            })
            .when_some(destination, |row, destination| {
                row.drag_over::<DraggedNote>(move |style, _, _, _| style.bg(selection))
                    .on_drop::<DraggedNote>(move |note, _, cx| {
                        cx.stop_propagation();
                        drop_state.update(cx, |state, cx| {
                            if state.window_label() != note.window
                                || path_key(&dirname(&note.path)) == path_key(&destination)
                            {
                                return;
                            }
                            if let Err(error) = state.services().workspace.move_document(
                                state.window_label(),
                                &note.path,
                                &destination,
                                None,
                            ) {
                                state.set_message(error, cx);
                            }
                        });
                    })
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                context_menu(&menu_path, directory, root, favourite, marked, cx).show(
                    event.position,
                    window,
                    cx,
                );
            })
            .child(
                Button::new("entry")
                    .child(caption)
                    .accessibility_label(name)
                    .tooltip(path)
                    .ghost()
                    .compact()
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .min_w(px(0.))
                    .when(selected, |button| button.bg(selection))
                    .on_click(cx.listener(move |sidebar, _, _, cx| {
                        match self.kind {
                            EntryKind::Folder { root, .. } => {
                                let expanded = if root {
                                    &mut sidebar.collapsed_roots
                                } else {
                                    &mut sidebar.expanded
                                };
                                if !expanded.remove(&open) {
                                    expanded.insert(open.clone());
                                }
                                cx.notify();
                            }
                            EntryKind::Favourite => {
                                sidebar.collapsed_roots.remove(&open);
                                sidebar.expanded.insert(open.clone());
                                sidebar
                                    .state
                                    .update(cx, |state, cx| state.open_favourite(&open, false, cx));
                            }
                            EntryKind::Document => sidebar
                                .state
                                .update(cx, |state, cx| state.open_document(&open, cx)),
                        }
                    })),
            )
            .child(
                Button::new("bookmark")
                    .icon(if marked {
                        IconName::BookmarkCheck
                    } else {
                        IconName::Bookmark
                    })
                    .ghost()
                    .compact()
                    .toggled(marked)
                    .tooltip(mark_title)
                    .accessibility_label(mark_title)
                    .on_click(move |_, window, cx| {
                        window.dispatch_action(
                            Box::new(FileAction {
                                path: bookmark.clone(),
                                operation: FileOperation::Bookmark,
                            }),
                            cx,
                        )
                    }),
            )
            .when(root, |row| {
                row.child(
                    Button::new("close-root")
                        .icon(IconName::X)
                        .ghost()
                        .compact()
                        .tooltip(tr(cx, "Close Folder"))
                        .accessibility_label(tr(cx, "Close Folder"))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(
                                Box::new(FileAction {
                                    path: close.clone(),
                                    operation: FileOperation::CloseRoot,
                                }),
                                cx,
                            )
                        }),
                )
            })
            .into_any_element()
    }
}

fn context_menu(
    path: &str,
    directory: bool,
    root: bool,
    favourite: bool,
    marked: bool,
    cx: &App,
) -> NativeMenu {
    let add = |menu: NativeMenu, label, icon, operation| {
        menu.menu_with_icon(
            tr(cx, label),
            icon,
            Box::new(FileAction {
                path: path.to_owned(),
                operation,
            }),
        )
    };
    let mut menu = NativeMenu::new();
    if directory {
        menu = add(
            menu,
            "New Document",
            IconName::FilePlus,
            FileOperation::NewDocument,
        );
        menu = add(
            menu,
            "New Folder",
            IconName::FolderPlus,
            FileOperation::NewFolder,
        );
        if root {
            menu = add(menu, "Close Folder", IconName::X, FileOperation::CloseRoot);
        }
    } else {
        menu = add(menu, "Open", IconName::FileText, FileOperation::Open);
        menu = add(
            menu,
            "Open in New Tab",
            IconName::SquareArrowOutUpRight,
            FileOperation::OpenInTab,
        );
        menu = add(menu, "Duplicate", IconName::Copy, FileOperation::Duplicate);
        menu = add(
            menu,
            "Move to Folder",
            IconName::FolderInput,
            FileOperation::Move,
        );
    }
    if !root {
        menu = add(menu, "Rename", IconName::Pencil, FileOperation::Rename);
    }
    menu = add(
        menu.separator(),
        "Reveal in File Manager",
        IconName::FolderOpen,
        FileOperation::Reveal,
    );
    menu = add(
        menu,
        "Copy Path",
        IconName::Clipboard,
        FileOperation::CopyPath,
    );
    menu = add(
        menu,
        if marked {
            "Remove from Favourites"
        } else {
            "Add to Favourites"
        },
        IconName::Bookmark,
        FileOperation::Bookmark,
    );
    if !root && !favourite {
        menu = add(
            menu.separator(),
            "Move to Trash",
            IconName::Trash,
            FileOperation::Trash,
        );
    }
    menu
}

impl SidebarPanel {
    pub(super) fn render_files(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.read(cx);
        let values = state.favourites();
        let roots = state.folders().to_vec();
        let favourites: HashSet<_> = values.iter().map(|path| path_key(path)).collect();
        let mut entries = Vec::new();
        for path in &roots {
            let open = !self.collapsed_roots.contains(path);
            entries.push(Entry {
                path: path.clone(),
                depth: 0,
                kind: EntryKind::Folder { open, root: true },
            });
            if open {
                walk(
                    state,
                    path,
                    1,
                    &self.expanded,
                    &mut HashSet::new(),
                    &mut entries,
                );
            }
        }

        let mut view = div()
            .v_flex()
            .child(div().px_3().pb_1().child(Label::new(tr(cx, "FAVOURITES"))));
        for path in values {
            view = view.child(
                Entry {
                    path,
                    depth: 0,
                    kind: EntryKind::Favourite,
                }
                .row(&self.state, &favourites, cx),
            );
        }
        if entries.is_empty() {
            view = view.child(
                div().px_3().py_2().child(
                    Button::new("add-first-folder")
                        .icon(IconName::FolderPlus)
                        .label(tr(cx, "Open folder"))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenFolder), cx)),
                ),
            );
        }
        for entry in entries {
            view = view.child(entry.row(&self.state, &favourites, cx));
        }
        view.into_any_element()
    }
}

fn walk(
    state: &WorkspaceState,
    dir: &str,
    depth: usize,
    expanded: &HashSet<String>,
    seen: &mut HashSet<std::path::PathBuf>,
    entries: &mut Vec<Entry>,
) {
    if depth > 32 || entries.len() >= 5000 {
        return;
    }
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return;
    };
    if !seen.insert(canonical) {
        return;
    }
    let Ok(children) = calmmd_core::tree::list(Path::new(dir), false) else {
        return;
    };
    for child in children {
        if entries.len() >= 5000 {
            break;
        }
        if !child.is_dir && !calmmd_core::core::paths::is_markdown_path(&child.name) {
            continue;
        }
        if state
            .services()
            .workspace
            .resolve(state.window_label(), &child.path)
            .is_err()
        {
            continue;
        }
        let open = child.is_dir && expanded.contains(&child.path);
        let kind = if child.is_dir {
            EntryKind::Folder { open, root: false }
        } else {
            EntryKind::Document
        };
        entries.push(Entry {
            path: child.path.clone(),
            depth,
            kind,
        });
        if open {
            walk(state, &child.path, depth + 1, expanded, seen, entries);
        }
    }
}
