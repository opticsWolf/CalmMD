use super::*;
use std::rc::Rc;

fn window_command<A: Action>(
    cx: &mut App,
    windows: &WindowList,
    run: fn(Entity<AppView>, &mut Window, &mut App),
) {
    let windows = windows.clone();
    cx.on_action(move |_: &A, cx| {
        if let Some(entry) = active_entry(&windows, cx) {
            after_window_action(cx, entry.handle, move |window, cx| {
                run(entry.view, window, cx)
            });
        }
    });
}

pub(super) fn register(cx: &mut App, windows: &WindowList) {
    window_command::<RecentFolders>(cx, windows, recent_folders);
    window_command::<Spaces>(cx, windows, spaces);
    window_command::<GoToLine>(cx, windows, goto_line);
    window_command::<CommandPalette>(cx, windows, command_palette);
    window_command::<FilePalette>(cx, windows, file_palette);
    window_command::<HeadingPalette>(cx, windows, heading_palette);
    window_command::<Appearance>(cx, windows, appearance);
    window_command::<CustomizeToolbar>(cx, windows, toolbar::customize);
    window_command::<ImageFile>(cx, windows, image_file);
    window_command::<ImageLocation>(cx, windows, image_location);
    window_command::<ToggleSpelling>(cx, windows, |view, _, cx| {
        let state = view.read(cx).state.clone();
        state.update(cx, |state, cx| {
            let language = state.spell_language().is_none().then(|| "auto".to_owned());
            state.set_spell_language(language, cx);
        });
    });
    let jump_windows = windows.clone();
    cx.on_action(move |action: &JumpToLine, cx| {
        if let Some(entry) = active_entry(&jump_windows, cx) {
            jump_to_line(&entry.view, entry.handle, action.0, cx);
        }
    });
    window_command::<ParentFolder>(cx, windows, |view, _, cx| {
        let state = view.read(cx).state.clone();
        let parent = state
            .read(cx)
            .root()
            .and_then(|root| Path::new(root).parent())
            .map(Path::to_path_buf);
        if let Some(parent) = parent {
            state.update(cx, |state, cx| state.open_folder(parent, cx));
        }
    });
    window_command::<CloseFolder>(cx, windows, |view, _, cx| {
        view.read(cx)
            .state
            .clone()
            .update(cx, |state, cx| state.close_folder(cx));
    });
    window_command::<ReopenDocument>(cx, windows, |view, _, cx| {
        view.read(cx)
            .state
            .clone()
            .update(cx, |state, cx| state.reopen_document(cx));
    });
    window_command::<ToggleSidebar>(cx, windows, |view, _, cx| {
        view.update(cx, |view, cx| {
            view.sidebar_open = !view.sidebar_open;
            cx.notify();
        });
    });
    window_command::<ToggleToolbar>(cx, windows, |view, _, cx| {
        view.update(cx, |view, cx| {
            view.toolbar_visible = !view.toolbar_visible;
            cx.notify();
        });
    });
    window_command::<ZoomIn>(cx, windows, |view, _, cx| zoom(view, cx, |z| z + 0.1));
    window_command::<ZoomOut>(cx, windows, |view, _, cx| zoom(view, cx, |z| z - 0.1));
    window_command::<ZoomReset>(cx, windows, |view, _, cx| zoom(view, cx, |_| 1.0));
    window_command::<HelpShortcuts>(cx, windows, |_, window, cx| {
        help(
            window,
            cx,
            "Keyboard Shortcuts",
            &[
                "Ctrl/Cmd+Z: Undo",
                "Ctrl+Y or Ctrl/Cmd+Shift+Z: Redo",
                "Ctrl/Cmd+F: Find in Document",
                "Ctrl/Cmd+Shift+F: Search in Folder",
                "Ctrl/Cmd+S: Save",
                "Ctrl/Cmd+W: Close Document",
            ],
        )
    });
    window_command::<HelpWelcome>(cx, windows, |_, window, cx| {
        help(
            window,
            cx,
            "Welcome to CalmMD",
            &[
                "Open a folder to browse your Markdown files.",
                "Use Split view to write beside the preview.",
                "Use File > Spaces to save a folder and its open documents.",
            ],
        )
    });
    window_command::<HelpMarkdown>(cx, windows, |_, window, cx| {
        help(
            window,
            cx,
            "Markdown Guide",
            &[
                "# Heading",
                "**bold**  *italic*  ~~strikethrough~~",
                "- List item",
                "- [ ] Task",
                "[Link](destination)",
                "> Quotation",
                "Use Insert for tables, code blocks, math, and diagrams.",
            ],
        )
    });
    register_insert_dialog::<ImageLink>(
        cx,
        windows,
        "Insert Image Link",
        "Image URL or relative path",
        "",
        |path, selection| {
            if path.is_empty() || path.chars().any(char::is_control) {
                return Err("Enter an image URL or path.".into());
            }
            let alt = selection.replace(['\r', '\n'], " ").replace(']', r"\]");
            Ok(format!(
                "![{alt}](<{}>)",
                path.replace('<', "%3C").replace('>', "%3E")
            ))
        },
    );
    register_insert_dialog::<InsertFootnote>(
        cx,
        windows,
        "Insert Footnote",
        "Footnote text",
        "",
        |value, selection| {
            if value.is_empty() {
                return Err("Enter the footnote text.".into());
            }
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            Ok(format!(
                "{selection}[^note-{id}]\n\n[^note-{id}]: {value}\n"
            ))
        },
    );
    register_insert_dialog::<ChooseHeading>(
        cx,
        windows,
        "Heading Level",
        "Level 1 to 6",
        "2",
        |level, selection| {
            let level = level
                .parse::<usize>()
                .ok()
                .filter(|level| (1..=6).contains(level))
                .ok_or("Enter a heading level from 1 to 6.")?;
            Ok(format!("\n{} {selection}\n", "#".repeat(level)))
        },
    );

    let [palette, line, reopen, larger, smaller, reset] = if cfg!(target_os = "macos") {
        [
            "cmd-shift-p",
            "cmd-g",
            "cmd-shift-t",
            "cmd-=",
            "cmd--",
            "cmd-0",
        ]
    } else {
        [
            "ctrl-shift-p",
            "ctrl-g",
            "ctrl-shift-t",
            "ctrl-=",
            "ctrl--",
            "ctrl-0",
        ]
    };
    cx.bind_keys([
        KeyBinding::new(palette, CommandPalette, None),
        KeyBinding::new(line, GoToLine, None),
        KeyBinding::new(reopen, ReopenDocument, None),
        KeyBinding::new(larger, ZoomIn, None),
        KeyBinding::new(smaller, ZoomOut, None),
        KeyBinding::new(reset, ZoomReset, None),
    ]);
}

fn zoom(view: Entity<AppView>, cx: &mut App, change: fn(f32) -> f32) {
    view.update(cx, |view, cx| {
        view.zoom = change(view.zoom).clamp(0.6, 2.0);
        let zoom = view.zoom;
        view.editor.update(cx, |panel, cx| {
            panel.zoom = zoom;
            cx.notify();
        });
        view.preview.update(cx, |panel, cx| {
            panel.zoom = zoom;
            cx.notify();
        });
        cx.notify();
    });
}

fn goto_line(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let state = view.read(cx).state.clone();
    if state.read(cx).active().is_none() {
        return;
    }
    crate::menu_dialogs::text_prompt(
        window,
        cx,
        "Go to Line",
        "Line number",
        "",
        move |value, window, cx| {
            let count = state.read(cx).text().split('\n').count();
            let line = value
                .parse::<usize>()
                .ok()
                .filter(|line| (1..=count).contains(line))
                .ok_or_else(|| format!("Enter a line number from 1 to {count}."))?;
            jump_to_line(&view, window.window_handle(), line - 1, cx);
            Ok(())
        },
    );
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = calmmd, no_json)]
struct JumpToLine(usize);

pub(super) fn jump_in_window(window: &mut Window, cx: &mut App, line: usize) {
    window.dispatch_action(Box::new(JumpToLine(line)), cx);
}

fn jump_to_line(view: &Entity<AppView>, handle: AnyWindowHandle, line: usize, cx: &mut App) {
    let editor = view.update(cx, |view, cx| {
        if view.view_mode == ViewMode::Preview {
            view.view_mode = ViewMode::Split;
            cx.notify();
        }
        view.editor.clone()
    });
    let input = editor.read(cx).editor.input.clone();
    after_window_action(cx, handle, move |window, _| {
        window.on_next_frame(move |window, cx| {
            input.update(cx, |editor, cx| {
                editor.set_cursor_position(
                    gpui_kit::component::input::Position::new(line as u32, 0),
                    window,
                    cx,
                )
            });
            input.read(cx).focus_handle(cx).focus(window, cx);
        });
    });
}

fn file_palette(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let state = view.read(cx).state.clone();
    let entries = state
        .read(cx)
        .documents()
        .iter()
        .map(|doc| {
            let path = doc.path.clone();
            let root = state.read(cx).root().map(Path::new);
            let label = root
                .and_then(|root| Path::new(&path).strip_prefix(root).ok())
                .unwrap_or_else(|| Path::new(&path))
                .display()
                .to_string();
            let state = state.clone();
            let run: ChoiceAction =
                Rc::new(move |_, cx| state.update(cx, |state, cx| state.open_document(&path, cx)));
            (label, run)
        })
        .collect();
    choices(window, cx, "Go to File", entries);
}

fn heading_palette(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let state = view.read(cx).state.read(cx);
    let entries = state
        .active()
        .and_then(|_| calmmd_core::core::markdown::parse_document(state.text()).ok())
        .map(|doc| {
            doc.headings
                .into_iter()
                .map(|heading| {
                    let view = view.clone();
                    let run: ChoiceAction = Rc::new(move |window, cx| {
                        jump_to_line(&view, window.window_handle(), heading.line, cx)
                    });
                    (
                        format!("{} {}", "#".repeat(heading.level as usize), heading.text),
                        run,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    choices(window, cx, "Go to Heading", entries);
}

fn appearance(_: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let entries: Vec<Choice> = vec![
        ("Light".into(), Rc::new(|_, cx| theme::set(cx, false))),
        ("Dark".into(), Rc::new(|_, cx| theme::set(cx, true))),
        (
            "Match System".into(),
            Rc::new(|_, cx| {
                theme::set(
                    cx,
                    matches!(
                        cx.window_appearance(),
                        WindowAppearance::Dark | WindowAppearance::VibrantDark
                    ),
                )
            }),
        ),
    ];
    choices(window, cx, "Appearance", entries);
}

fn help(window: &mut Window, cx: &mut App, title: &'static str, lines: &'static [&'static str]) {
    window.open_dialog(cx, move |dialog, _, _| {
        let content = div()
            .v_flex()
            .gap_2()
            .children(lines.iter().map(|line| Label::new(*line)));
        dialog.title(title).child(content).footer(
            Button::new("close-help")
                .label("Close")
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
    });
}

type ChoiceAction = Rc<dyn Fn(&mut Window, &mut App)>;
type Choice = (String, ChoiceAction);

fn choices(window: &mut Window, cx: &mut App, title: &'static str, entries: Vec<Choice>) {
    let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));
    let handle = window.window_handle();
    let subscription = cx.subscribe(
        &filter,
        move |_, event: &gpui_kit::component::input::InputEvent, cx| {
            if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                after_window_action(cx, handle, |window, _| window.refresh());
            }
        },
    );
    let first_render = std::cell::Cell::new(true);
    window.open_dialog(cx, move |dialog, window, cx| {
        let _subscription = &subscription;
        if first_render.replace(false) {
            filter.read(cx).focus_handle(cx).focus(window, cx);
        }
        let query = filter.read(cx).value().to_lowercase();
        let matching = entries
            .iter()
            .enumerate()
            .filter(|(_, (label, _))| label.to_lowercase().contains(&query));
        let empty = matching.clone().next().is_none();
        let rows = div()
            .id("choice-rows")
            .v_flex()
            .gap_1()
            .max_h(px(360.))
            .overflow_y_scroll()
            .children(matching.map(|(index, (label, run))| {
                let run = run.clone();
                Button::new(("choice", index))
                    .label(label.clone())
                    .on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        let run = run.clone();
                        window.defer(cx, move |window, cx| run(window, cx));
                    })
            }))
            .when(empty, |rows| rows.child(Label::new("No matches")));
        dialog.title(title).child(
            div()
                .v_flex()
                .gap_2()
                .child(Input::new(&filter))
                .child(rows),
        )
    });
}

fn command_palette(_: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let mut entries = Vec::new();
    for menu in app_menus() {
        collect_menu_commands(menu, &mut entries);
    }
    choices(window, cx, "Command Palette", entries);
}

fn collect_menu_commands(menu: Menu, entries: &mut Vec<Choice>) {
    for item in menu.items {
        match item {
            MenuItem::Submenu(menu) => collect_menu_commands(menu, entries),
            MenuItem::Action {
                name,
                action,
                disabled: false,
                ..
            } if !action.as_any().is::<CommandPalette>() => {
                if entries.iter().any(|(label, _)| label == name.as_ref()) {
                    continue;
                }
                let run: ChoiceAction =
                    Rc::new(move |window, cx| window.dispatch_action(action.boxed_clone(), cx));
                entries.push((name.to_string(), run));
            }
            _ => {}
        }
    }
}

fn recent_folders(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let state = view.read(cx).state.clone();
    let mut entries: Vec<_> = state
        .read(cx)
        .recent_folders()
        .into_iter()
        .map(|path| {
            let state = state.clone();
            let label = path.clone();
            let run: ChoiceAction = Rc::new(move |_, cx| {
                state.update(cx, |state, cx| {
                    if Path::new(&path).is_dir() {
                        state.open_folder(path.clone().into(), cx);
                    } else {
                        state.set_message("This folder is unavailable.".into(), cx);
                    }
                });
            });
            (label, run)
        })
        .collect();
    if !entries.is_empty() {
        entries.push((
            "Clear Recent Folders".into(),
            Rc::new(move |_, cx| {
                state.update(cx, |state, cx| {
                    let message = state
                        .clear_recent_folders()
                        .err()
                        .unwrap_or_else(|| "Recent folders cleared".into());
                    state.set_message(message, cx);
                });
            }),
        ));
    }
    choices(window, cx, "Open Recent Folder", entries);
}

fn spaces(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let state = view.read(cx).state.clone();
    let current = state.read(cx).space_name().unwrap_or_default();
    let mut entries: Vec<Choice> = Vec::new();
    let save_state = state.clone();
    entries.push((
        "Save Current Space...".into(),
        Rc::new(move |window, cx| {
            let state = save_state.clone();
            crate::menu_dialogs::text_prompt(
                window,
                cx,
                "Save Space",
                "Space name",
                &current,
                move |name, _, cx| state.update(cx, |state, cx| state.save_space(name, cx)),
            );
        }),
    ));
    let close_state = state.clone();
    entries.push((
        "Close Space".into(),
        Rc::new(move |_, cx| {
            close_state.update(cx, |state, cx| state.close_folder(cx));
        }),
    ));
    let saved = state.read(cx).services().session.spaces();
    match saved {
        Ok(saved) => {
            for space in saved {
                let open_state = state.clone();
                let id = space.id.clone();
                entries.push((
                    format!("Open {}", space.name),
                    Rc::new(move |_, cx| {
                        open_state.update(cx, |state, cx| {
                            if let Err(error) = state.switch_space(&id, cx) {
                                state.set_message(error, cx);
                            }
                        });
                    }),
                ));
                let delete_state = state.clone();
                entries.push((
                    format!("Remove {}...", space.name),
                    Rc::new(move |window, cx| {
                        let state = delete_state.clone();
                        let id = space.id.clone();
                        let name = space.name.clone();
                        crate::menu_dialogs::text_prompt(
                            window,
                            cx,
                            "Remove Space",
                            "Type the Space name to remove its saved workspace",
                            "",
                            move |input, _, cx| {
                                if input != name {
                                    return Err("The Space name does not match.".into());
                                }
                                state.update(cx, |state, cx| state.delete_space(&id, cx))
                            },
                        );
                    }),
                ));
            }
        }
        Err(error) => state.update(cx, |state, cx| state.set_message(error, cx)),
    }
    choices(window, cx, "Spaces", entries);
}

#[derive(Clone, Copy)]
struct ExportOption {
    section: &'static str,
    key: &'static str,
    label: &'static str,
    values: &'static [&'static str],
}

pub(super) fn export_options(view: Entity<AppView>, window: &mut Window, cx: &mut App, pdf: bool) {
    let state = view.read(cx).state.clone();
    let Some(path) = state.read(cx).active().map(str::to_owned) else {
        state.update(cx, |state, cx| {
            state.set_message("Open a document before exporting.".into(), cx)
        });
        return;
    };
    let prefs = Rc::new(std::cell::RefCell::new(
        calmmd_core::core::export::prefs::resolve_prefs(None),
    ));
    let author = cx.new(|cx| InputState::new(window, cx).placeholder("Cover author"));
    let pdf_options = [
        ExportOption {
            section: "pdf",
            key: "pageSize",
            label: "Paper size",
            values: &["A4", "Letter", "A5", "Legal"],
        },
        ExportOption {
            section: "pdf",
            key: "margins",
            label: "Margins",
            values: &["normal", "narrow", "wide"],
        },
        ExportOption {
            section: "pdf",
            key: "template",
            label: "Template",
            values: &["standard", "report", "academic", "minimal"],
        },
        ExportOption {
            section: "pdf",
            key: "header",
            label: "Running header",
            values: &[],
        },
        ExportOption {
            section: "pdf",
            key: "pageNumbers",
            label: "Page numbers",
            values: &[],
        },
        ExportOption {
            section: "pdf",
            key: "breakBeforeH1",
            label: "Start top-level headings on a new page",
            values: &[],
        },
    ];
    let common_options = [
        ExportOption {
            section: "",
            key: "toc",
            label: "Table of contents",
            values: &[],
        },
        ExportOption {
            section: "",
            key: "embedImages",
            label: "Embed local images",
            values: &[],
        },
        ExportOption {
            section: "",
            key: "cover",
            label: "Cover page",
            values: &[],
        },
    ];
    window.open_dialog(cx, move |dialog, _, _| {
        let mut content = div().v_flex().gap_2();
        let options = pdf_options.iter().filter(|_| pdf).chain(common_options.iter().filter(|option| !pdf || option.key != "embedImages"));
        for &option in options {
            let value = if option.section.is_empty() { prefs.borrow()[option.key].clone() }
                else { prefs.borrow()[option.section][option.key].clone() };
            let mut row = div().h_flex().flex_wrap().justify_between().gap_1().child(Label::new(option.label));
            if option.values.is_empty() {
                let prefs = prefs.clone();
                row = div().h_flex().child(gpui_kit::component::checkbox::Checkbox::new(option.key)
                    .label(option.label).checked(value.as_bool().unwrap_or(false))
                    .on_click(move |checked, window, _| set_export_option(&prefs, option, Value::Bool(*checked), window)));
            } else {
                let mut selections = div().h_flex().gap_1();
                for (index, &candidate) in option.values.iter().enumerate() {
                    let prefs = prefs.clone();
                    selections = selections.child(Button::new((option.key, index)).label(candidate).compact()
                        .when(value.as_str() == Some(candidate), |button| button.primary())
                        .on_click(move |_, window, _| set_export_option(&prefs, option, Value::from(candidate), window)));
                }
                row = row.child(selections);
            }
            content = content.child(row);
        }
        if prefs.borrow()["cover"].as_bool() == Some(true) {
            content = content.child(Input::new(&author));
        }
        let export_prefs = prefs.clone();
        let path = path.clone();
        let state = state.clone();
        let view = view.clone();
        let author = author.clone();
        dialog.title(if pdf { "Export PDF" } else { "Export HTML" }).child(content)
            .footer(div().h_flex().justify_end().gap_2()
                .child(Button::new("cancel-export").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                .child(Button::new("confirm-export").label("Export...").primary().on_click(move |_, window, cx| {
                    if state.read(cx).active() != Some(path.as_str()) {
                        state.update(cx, |state, cx| state.set_message("The active document changed. Reopen Export to try again.".into(), cx));
                        window.close_dialog(cx);
                        return;
                    }
                    let mut options = export_prefs.borrow().clone();
                    options["coverAuthor"] = Value::from(author.read(cx).value().trim().to_owned());
                    window.close_dialog(cx);
                    export_document(&view, pdf, options, Value::Null, cx);
                })))
    });
}

fn set_export_option(
    prefs: &std::cell::RefCell<Value>,
    option: ExportOption,
    value: Value,
    window: &mut Window,
) {
    let mut prefs = prefs.borrow_mut();
    if option.section.is_empty() {
        prefs[option.key] = value;
    } else {
        prefs[option.section][option.key] = value;
    }
    window.refresh();
}

fn image_location(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let mut entries = Vec::new();
    for (label, root) in [
        ("Next to the document (assets/)", false),
        ("In the folder or Space root (assets/)", true),
    ] {
        let view = view.clone();
        let run: ChoiceAction = Rc::new(move |_, cx| {
            let state = view.read(cx).state.clone();
            let result = calmmd_core::kv::write(
                state.read(cx).context(),
                "gpui/image-location",
                if root { "root" } else { "document" },
            );
            if let Err(error) = result {
                state.update(cx, |state, cx| state.set_message(error, cx));
                return;
            }
            view.update(cx, |view, cx| {
                view.images_in_root = root;
                cx.notify();
            });
        });
        entries.push((label.to_owned(), run));
    }
    choices(window, cx, "Image Asset Location", entries);
}

fn image_file(view: Entity<AppView>, window: &mut Window, cx: &mut App) {
    let app = view.read(cx);
    let state = app.state.clone();
    let Some(path) = state.read(cx).active().map(str::to_owned) else {
        state.update(cx, |state, cx| {
            state.set_message("Open a document before inserting an image.".into(), cx)
        });
        return;
    };
    let root = app
        .images_in_root
        .then(|| state.read(cx).root().map(str::to_owned))
        .flatten();
    let input = app.editor.read(cx).editor.input.clone();
    let platform = app.platform.clone();
    let original = input.read(cx).value();
    let selection = input.read(cx).selected_range();
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let Some(source) = platform.open_image() else {
            return;
        };
        let source_path = path.clone();
        let result = cx
            .background_executor()
            .spawn(async move {
                import_image(
                    &source,
                    Path::new(&source_path),
                    root.as_deref().map(Path::new),
                )
            })
            .await;
        let _ = cx.update(|cx| match result {
            Ok(link) => after_window_action(cx, handle, move |window, cx| {
                if state.read(cx).active() != Some(path.as_str())
                    || input.read(cx).value() != original
                {
                    state.update(cx, |state, cx| {
                        state.set_message(
                            format!("Image copied to {link}. The active document changed."),
                            cx,
                        )
                    });
                    return;
                }
                input.update(cx, |editor, cx| {
                    let alt = original
                        .get(selection.clone())
                        .unwrap_or_default()
                        .replace(['\r', '\n'], " ")
                        .replace(']', r"\]");
                    editor.set_selected_range(selection, cx);
                    editor.replace(format!("![{alt}](<{link}>)"), window, cx);
                });
            }),
            Err(error) => state.update(cx, |state, cx| state.set_message(error, cx)),
        });
    })
    .detach();
}

fn import_image(source: &Path, document: &Path, root: Option<&Path>) -> Result<String, String> {
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif",
    ]
    .contains(&extension.as_str())
    {
        return Err("Select a supported image file.".into());
    }
    let parent = document
        .parent()
        .ok_or("The document has no parent directory.")?;
    let root = root
        .filter(|root| parent.starts_with(root))
        .unwrap_or(parent);
    let back_steps = parent
        .strip_prefix(root)
        .map_or(0, |relative| relative.components().count());
    let folder = root.join("assets");
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create assets: {error}"))?;
    if !std::fs::symlink_metadata(&folder)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("The assets path must be a directory.".into());
    }
    let name = source
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let safe_name: String = name
        .chars()
        .take(80)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = format!("image-{safe_name}-{id}.{extension}");
    calmmd_core::fsops::copy_file(
        source.to_string_lossy().into_owned(),
        folder.join(&base).to_string_lossy().into_owned(),
    )?;
    Ok(format!("{}assets/{base}", "../".repeat(back_steps)))
}
