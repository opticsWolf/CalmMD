//! CalmMD palettes translated from `src/styles/tokens.css` into GPUI Kit themes.
//!
//! Only the colours CalmMD actually uses are overridden; Kit's component
//! tokens fill in the rest so buttons, lists and dialogs stay coherent.

use gpui_kit::{App, Hsla, component::theme::{Theme, ThemeMode}, px, rgb};

fn translucent(color: u32, alpha: f32) -> Hsla {
    Hsla {
        a: alpha,
        ..rgb(color).into()
    }
}

/// Installs the Snow/Paper light palette as the initial theme.
pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Light, None, cx);
    apply(cx, ThemeMode::Light);
}

/// Switches between the Snow/Paper light and Ink dark palettes.
pub fn toggle(cx: &mut App) {
    set(cx, !Theme::global(cx).mode.is_dark());
}

/// Selects a palette explicitly, also used by screenshot automation.
pub fn set(cx: &mut App, dark: bool) {
    let mode = if dark { ThemeMode::Dark } else { ThemeMode::Light };
    Theme::change(mode, None, cx);
    apply(cx, mode);
}

fn apply(cx: &mut App, mode: ThemeMode) {
    let dark = mode.is_dark();
    Theme::update(cx, |theme| {
        theme.mode = mode;
        theme.font_size = px(18.0);
        theme.mono_font_size = px(16.0);
        theme.radius = px(8.0);
        theme.radius_lg = px(10.0);

        theme.background = if dark { rgb(0x131722).into() } else { rgb(0xfaf8f4).into() };
        theme.foreground = if dark { rgb(0xe6e7eb).into() } else { rgb(0x2a2d34).into() };
        theme.muted = if dark { rgb(0x202737).into() } else { rgb(0xf2efe8).into() };
        theme.muted_foreground = if dark { rgb(0x9aa3b2).into() } else { rgb(0x5f6471).into() };
        theme.border = if dark { rgb(0x303746).into() } else { rgb(0xe4ded2).into() };
        theme.accent = rgb(0xe8a23b).into();
        theme.accent_foreground = rgb(0x1f2a44).into();
        theme.link = if dark { rgb(0x8ab4ff).into() } else { rgb(0x2f5e9e).into() };
        theme.caret = if dark { rgb(0xe8a23b).into() } else { rgb(0x1f2a44).into() };
        theme.selection = translucent(0xe8a23b, 0.26);
        theme.button = if dark { rgb(0x202737).into() } else { rgb(0xf2efe8).into() };
        theme.button_foreground = theme.foreground;
        theme.input = theme.background;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_helper_preserves_opacity() {
        let color = translucent(0xe8a23b, 0.26);
        assert!((color.a - 0.26).abs() < f32::EPSILON);
    }
}
