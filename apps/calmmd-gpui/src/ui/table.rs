use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Action)]
#[action(namespace = calmmd, no_json)]
pub(super) enum TableEdit {
    RowAbove,
    RowBelow,
    ColumnLeft,
    ColumnRight,
    DeleteRow,
    DeleteColumn,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Format,
}

impl TableEdit {
    const ITEMS: [(&'static str, Self); 10] = [
        ("Insert Row Above", Self::RowAbove),
        ("Insert Row Below", Self::RowBelow),
        ("Insert Column Left", Self::ColumnLeft),
        ("Insert Column Right", Self::ColumnRight),
        ("Delete Row", Self::DeleteRow),
        ("Delete Column", Self::DeleteColumn),
        ("Align Column Left", Self::AlignLeft),
        ("Align Column Center", Self::AlignCenter),
        ("Align Column Right", Self::AlignRight),
        ("Format Table", Self::Format),
    ];

    pub(super) fn menu() -> MenuItem {
        MenuItem::submenu(
            Menu::new("Table")
                .items(Self::ITEMS.map(|(label, action)| MenuItem::action(label, action))),
        )
    }
}

pub(super) fn register(cx: &mut App, windows: &WindowList) {
    let windows = windows.clone();
    cx.on_action(move |action: &TableEdit, cx| {
        let Some(entry) = active_entry(&windows, cx) else {
            return;
        };
        let action = *action;
        after_window_action(cx, entry.handle, move |window, cx| {
            let view = entry.view.read(cx);
            let state = view.state.clone();
            if state.read(cx).active().is_none() {
                return;
            }
            let input = view.editor.read(cx).editor.input.clone();
            let editor = input.read(cx);
            let value = editor.value();
            let cursor = editor.selected_range().start;
            match edit_table(&value, cursor, action) {
                Ok((range, replacement)) => input.update(cx, |editor, cx| {
                    editor.set_selected_range(range.clone(), cx);
                    editor.replace(replacement, window, cx);
                    editor.set_selected_range(range.start..range.start, cx);
                    editor.focus_handle(cx).focus(window, cx);
                }),
                Err(error) => state.update(cx, |state, cx| state.set_message(error.to_owned(), cx)),
            }
        });
    });
}

fn separators(row: &str) -> Vec<usize> {
    let mut separators = Vec::new();
    let mut chars = row.char_indices().peekable();
    let mut code = 0;
    while let Some((i, ch)) = chars.next() {
        match ch {
            '\\' if code == 0 => {
                chars.next();
            }
            '\x60' => {
                let mut count = 1;
                while chars.next_if(|(_, next)| *next == '\x60').is_some() {
                    count += 1;
                }
                if code == 0 {
                    code = count;
                } else if code == count {
                    code = 0;
                }
            }
            '|' if code == 0 => separators.push(i),
            _ => {}
        }
    }
    separators
}

fn row_cells(row: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let mut start = 0;
    for pipe in separators(row)
        .into_iter()
        .chain(std::iter::once(row.len()))
    {
        cells.push(row[start..pipe].trim());
        start = pipe + 1;
    }
    if row.trim_start().starts_with('|') {
        cells.remove(0);
    }
    if row.trim_end().ends_with('|') && cells.last() == Some(&"") {
        cells.pop();
    }
    cells
}

fn edit_table(
    source: &str,
    cursor: usize,
    action: TableEdit,
) -> Result<(std::ops::Range<usize>, String), &'static str> {
    const NO_TABLE: &str = "Place the cursor inside a Markdown table.";
    let cursor_line = source[..cursor].bytes().filter(|&b| b == b'\n').count();
    let parsed = calmmd_core::core::markdown::parse_document(source).map_err(|_| NO_TABLE)?;
    let block = parsed
        .blocks
        .into_iter()
        .find(|block| {
            block.kind.eq_ignore_ascii_case("table")
                && (block.start_line..=block.end_line).contains(&cursor_line)
        })
        .ok_or(NO_TABLE)?;
    let lines: Vec<_> = source.split('\n').collect();
    let first = block.start_line;
    let last = (first + 2..=block.end_line.min(lines.len() - 1))
        .take_while(|&line| !lines[line].trim().is_empty() && !separators(lines[line]).is_empty())
        .last()
        .unwrap_or(first + 1);
    if cursor_line > last {
        return Err(NO_TABLE);
    }

    let mut rows: Vec<_> = lines[first..=last]
        .iter()
        .map(|row| row_cells(row))
        .collect();
    let width = rows[0].len();
    if width == 0 || rows[1].len() != width {
        return Err(NO_TABLE);
    }
    for row in &mut rows {
        row.resize(width, "");
    }
    let row_index = cursor_line - first;
    let column_offset = source[..cursor]
        .rsplit_once('\n')
        .map_or(cursor, |(_, tail)| tail.len());
    let row = lines[cursor_line];
    let leading = usize::from(row.trim_start().starts_with('|'));
    let column = separators(row)
        .iter()
        .filter(|&&pipe| pipe < column_offset)
        .count()
        .saturating_sub(leading)
        .min(width - 1);
    match action {
        TableEdit::RowAbove => rows.insert(row_index.max(2), vec![""; width]),
        TableEdit::RowBelow => rows.insert((row_index + 1).max(2), vec![""; width]),
        TableEdit::DeleteRow if row_index <= 1 => {
            rows[0] = if rows.len() > 2 {
                rows.remove(2)
            } else {
                vec![""; width]
            };
        }
        TableEdit::DeleteRow => {
            rows.remove(row_index);
        }
        TableEdit::ColumnLeft | TableEdit::ColumnRight => {
            let column = column + usize::from(action == TableEdit::ColumnRight);
            for (i, row) in rows.iter_mut().enumerate() {
                row.insert(column, if i == 1 { "---" } else { "" });
            }
        }
        TableEdit::DeleteColumn => {
            if width == 1 {
                return Err("A Markdown table needs at least one column.");
            }
            for row in &mut rows {
                row.remove(column);
            }
        }
        TableEdit::AlignLeft => rows[1][column] = ":---",
        TableEdit::AlignCenter => rows[1][column] = ":---:",
        TableEdit::AlignRight => rows[1][column] = "---:",
        TableEdit::Format => {}
    }

    let widths: Vec<_> = (0..rows[0].len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(3)
                .max(3)
        })
        .collect();
    let edited_rows: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let cells: Vec<_> = row
                .iter()
                .zip(&widths)
                .map(|(cell, &width)| {
                    if index != 1 {
                        return format!("{cell:width$}");
                    }
                    let left = if cell.starts_with(':') { ":" } else { "" };
                    let right = if cell.ends_with(':') { ":" } else { "" };
                    format!(
                        "{left}{}{right}",
                        "-".repeat(width - left.len() - right.len())
                    )
                })
                .collect();
            format!("| {} |", cells.join(" | "))
        })
        .collect();
    let start = lines[..first]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>();
    let end = start
        + lines[first..=last]
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>()
        - 1;
    let newline = if lines[first].ends_with('\r') {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_cr = if lines[last].ends_with('\r') {
        "\r"
    } else {
        ""
    };
    Ok((
        start..end,
        format!("{}{trailing_cr}", edited_rows.join(newline)),
    ))
}

#[cfg(test)]
mod tests {
    use super::{edit_table, row_cells, TableEdit};

    #[test]
    fn table_controls_preserve_escaped_and_code_pipes() {
        assert_eq!(
            row_cells("| a\\|b | \x60\x60c|d\x60\x60 |"),
            [r"a\|b", "\x60\x60c|d\x60\x60"]
        );
        assert_eq!(row_cells(r"| a \|"), [r"a \|"]);
    }

    #[test]
    fn column_edit_replaces_only_the_table() {
        let source = "Intro\n\n| A | B |\n| - | - |\n| c | d |\n\nAfter";
        let cursor = source.find(" B ").unwrap() + 1;
        let (range, replacement) = edit_table(source, cursor, TableEdit::DeleteColumn).unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(edited, "Intro\n\n| A   |\n| --- |\n| c   |\n\nAfter");
        assert!(edit_table(source, source.find("After").unwrap(), TableEdit::Format).is_err());
    }
}
