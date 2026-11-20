//! GPUI Kit source editor for Markdown notes.
//!
//! The editor is a view, not the authority: text edits report into
//! `WorkspaceState`, which owns autosave, drafts, history and MCP-visible state.

use crate::state::WorkspaceState;
use calmmd_core::{AppContext, core::spell};
use gpui_kit::{
    component::input::{EditorState, InputEvent, TextDecoration, TextDecorationCollection},
    App, AppContext as _, Entity, HighlightStyle, Hsla, SharedString, Subscription, UnderlineStyle,
    Window, px, rgb,
};
use std::ops::Range;

/// Converts UTF-16 code-unit offsets from zspell into UTF-8 byte ranges used by GPUI decorations.
fn utf16_to_byte(text: &str, start_units: u32, end_units: u32) -> Option<Range<usize>> {
    if end_units < start_units {
        return None;
    }
    let mut units = 0u32;
    let mut start = None;
    let mut end = None;
    for (byte, ch) in text.char_indices() {
        let next = units + ch.len_utf16() as u32;
        if start.is_none() && units >= start_units {
            start = Some(byte);
        }
        if end.is_none() && units >= end_units {
            end = Some(byte);
        }
        if start.is_some() && end.is_some() {
            break;
        }
        units = next;
    }
    let start = start.unwrap_or(text.len()).min(text.len());
    let end = end.unwrap_or(text.len()).min(text.len());
    (start <= end).then_some(start..end)
}

fn spell_style() -> HighlightStyle {
    HighlightStyle {
        underline: Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(rgb(0xc2412d).into()),
            wavy: true,
        }),
        ..Default::default()
    }
}

fn search_style() -> HighlightStyle {
    HighlightStyle {
        background_color: Some(Hsla {
            a: 0.45,
            ..rgb(0xe8a23b).into()
        }),
        ..Default::default()
    }
}

/// Owns the GPUI editor state and its two decoration layers.
pub struct MarkdownEditor {
    pub input: Entity<EditorState>,
    spell_decorations: TextDecorationCollection,
    search_decorations: TextDecorationCollection,
    last_text: SharedString,
    last_decorated_text: SharedString,
    last_spell_language: Option<String>,
    last_search_query: String,
    _change: Subscription,
}

impl MarkdownEditor {
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        state: Entity<WorkspaceState>,
        text: &str,
    ) -> Self {
        let input = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false)
                .default_value(text)
        });
        let (spell_decorations, search_decorations) = input.update(cx, |editor, cx| {
            (
                editor.create_decorations_collection(Vec::new(), cx),
                editor.create_decorations_collection(Vec::new(), cx),
            )
        });
        let change_input = input.clone();
        let change_state = state.clone();
        let change = cx.subscribe(&input, move |_, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let value = change_input.read(cx).value().to_string();
            change_state.update(cx, |state, cx| state.editor_changed(value, cx));
        });

        let mut this = Self {
            input,
            spell_decorations,
            search_decorations,
            last_text: text.into(),
            last_decorated_text: "".into(),
            last_spell_language: None,
            last_search_query: String::new(),
            _change: change,
        };
        this.refresh_decorations(&state.read(cx).context().clone(), text, None, "", cx);
        this
    }

    /// Pushes an external text change into the editor without emitting another edit event.
    pub fn sync_text(&mut self, text: &str, window: &mut Window, cx: &mut App) {
        if self.last_text.as_ref() == text {
            return;
        }
        if self.input.read(cx).value().as_ref() != text {
            self.input.update(cx, |editor, cx| editor.set_value(text, window, cx));
        }
        self.last_text = text.into();
    }

    /// Recomputes spell and search decorations when their inputs change.
    pub fn refresh_decorations(
        &mut self,
        context: &AppContext,
        text: &str,
        spell_language: Option<&str>,
        search_query: &str,
        cx: &mut App,
    ) {
        let language_changed = self.last_spell_language.as_deref() != spell_language;
        let search_changed = self.last_search_query != search_query;
        let text_changed = self.last_decorated_text.as_ref() != text;
        if !language_changed && !search_changed && !text_changed {
            return;
        }
        self.last_spell_language = spell_language.map(str::to_owned);
        self.last_search_query = search_query.to_owned();
        self.last_decorated_text = text.into();

        let misspelled = spell_language
            .and_then(|language| spell::check_text(context, text, Some(language)).ok().flatten())
            .unwrap_or_default();
        let spell: Vec<TextDecoration> = misspelled
            .into_iter()
            .filter_map(|(start, end)| utf16_to_byte(text, start, end))
            .map(|range| TextDecoration::new(range, spell_style()))
            .collect();
        self.spell_decorations.set(spell, cx);

        let mut search = Vec::new();
        if !search_query.is_empty() {
            let needle = search_query.to_lowercase();
            let haystack = text.to_lowercase();
            let mut from = 0;
            while let Some(found) = haystack[from..].find(&needle) {
                let start = from + found;
                search.push(TextDecoration::new(
                    start..start + needle.len(),
                    search_style(),
                ));
                from = start + needle.len().max(1);
                if from >= haystack.len() {
                    break;
                }
            }
        }
        self.search_decorations.set(search, cx);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_map_through_multibyte_text() {
        let text = "Café 😀 recieve";
        let byte = text.find("recieve").unwrap();
        let units = text[..byte].encode_utf16().count() as u32;
        assert_eq!(
            utf16_to_byte(text, units, units + 7),
            Some(byte..byte + "recieve".len())
        );
    }

    #[test]
    fn utf16_offsets_clamp_to_the_document() {
        assert_eq!(utf16_to_byte("abc", 99, 200), Some(3..3));
        assert_eq!(utf16_to_byte("abc", 2, 1), None);
    }
}
