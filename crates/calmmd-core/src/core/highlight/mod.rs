//! CalmMD syntax themes for Mordant.
//!
//! The editor keeps its Lezer highlighting and CSS variables. This module mirrors the syntax
//! colours from `src/styles/tokens.css` into Mordant/syntect themes so the Rust Markdown service
//! can emit inline styles that match the active CalmMD theme. The themes are generated from one
//! palette table, so a token colour only has to be changed in Rust once and stays in sync with
//! the CSS palette it was copied from.

use std::sync::OnceLock;

use mordant::highlighter::{list_themes, register_custom_theme};
use serde_json::json;

pub const LIGHT_THEME: &str = "CalmMD Snow";
pub const DARK_THEME: &str = "CalmMD Ink";

#[derive(Clone, Copy)]
struct Palette {
    id: &'static str,
    name: &'static str,
    dark: bool,
    bg: &'static str,
    surface: &'static str,
    fg: &'static str,
    muted: &'static str,
    danger: &'static str,
    link: &'static str,
    accent: &'static str,
    caret: &'static str,
    comment: &'static str,
    keyword: &'static str,
    string: &'static str,
    number: &'static str,
    function: &'static str,
    type_name: &'static str,
    property: &'static str,
    tag: &'static str,
}

/// One entry per `ColorThemeId` in `src/app/colorThemes.ts`. Values come from `tokens.css`.
const PALETTES: &[Palette] = &[
    Palette {
        id: "snow",
        name: LIGHT_THEME,
        dark: false,
        bg: "#ffffff",
        surface: "#f5f6f8",
        fg: "#24272e",
        muted: "#565d6a",
        danger: "#b3261e",
        link: "#2459b3",
        accent: "#e8a23b",
        caret: "#1f2a44",
        comment: "#7d838e",
        keyword: "#7a3e9d",
        string: "#3f7a3a",
        number: "#a3561a",
        function: "#2f5e9e",
        type_name: "#8a6d1f",
        property: "#355d7a",
        tag: "#a33a3a",
    },
    Palette {
        id: "paper",
        name: "CalmMD Paper",
        dark: false,
        bg: "#faf8f4",
        surface: "#f2efe8",
        fg: "#2a2d34",
        muted: "#5f6471",
        danger: "#b3261e",
        link: "#2f5e9e",
        accent: "#e8a23b",
        caret: "#1f2a44",
        comment: "#7d838e",
        keyword: "#7a3e9d",
        string: "#3f7a3a",
        number: "#a3561a",
        function: "#2f5e9e",
        type_name: "#8a6d1f",
        property: "#355d7a",
        tag: "#a33a3a",
    },
    Palette {
        id: "github-light",
        name: "CalmMD GitHub Light",
        dark: false,
        bg: "#ffffff",
        surface: "#f6f8fa",
        fg: "#1f2328",
        muted: "#59636e",
        danger: "#d1242f",
        link: "#0969da",
        accent: "#0969da",
        caret: "#1f2328",
        comment: "#59636e",
        keyword: "#cf222e",
        string: "#0a3069",
        number: "#0550ae",
        function: "#8250df",
        type_name: "#953800",
        property: "#0550ae",
        tag: "#116329",
    },
    Palette {
        id: "code-light",
        name: "CalmMD Code Light",
        dark: false,
        bg: "#ffffff",
        surface: "#f8f8f8",
        fg: "#3b3b3b",
        muted: "#616161",
        danger: "#c72e0f",
        link: "#005fb8",
        accent: "#005fb8",
        caret: "#000000",
        comment: "#008000",
        keyword: "#0000ff",
        string: "#a31515",
        number: "#098658",
        function: "#795e26",
        type_name: "#267f99",
        property: "#001080",
        tag: "#800000",
    },
    Palette {
        id: "solarized-light",
        name: "CalmMD Solarized Light",
        dark: false,
        bg: "#fdf6e3",
        surface: "#eee8d5",
        fg: "#586e75",
        muted: "#657b83",
        danger: "#dc322f",
        link: "#268bd2",
        accent: "#b58900",
        caret: "#073642",
        comment: "#93a1a1",
        keyword: "#859900",
        string: "#2aa198",
        number: "#d33682",
        function: "#268bd2",
        type_name: "#b58900",
        property: "#6c71c4",
        tag: "#dc322f",
    },
    Palette {
        id: "ink",
        name: DARK_THEME,
        dark: true,
        bg: "#16191f",
        surface: "#1c2028",
        fg: "#d6dae1",
        muted: "#9aa1ad",
        danger: "#f2817a",
        link: "#8db4f0",
        accent: "#e8a23b",
        caret: "#e8a23b",
        comment: "#6f7684",
        keyword: "#c79bf2",
        string: "#a5d38f",
        number: "#f0a36b",
        function: "#8db4f0",
        type_name: "#e6c87a",
        property: "#9cc7e8",
        tag: "#f08f8f",
    },
    Palette {
        id: "github-dark",
        name: "CalmMD GitHub Dark",
        dark: true,
        bg: "#0d1117",
        surface: "#151b23",
        fg: "#d1d7e0",
        muted: "#9198a1",
        danger: "#f85149",
        link: "#4493f8",
        accent: "#4493f8",
        caret: "#f0f6fc",
        comment: "#9198a1",
        keyword: "#ff7b72",
        string: "#a5d6ff",
        number: "#79c0ff",
        function: "#d2a8ff",
        type_name: "#ffa657",
        property: "#79c0ff",
        tag: "#7ee787",
    },
    Palette {
        id: "code-dark",
        name: "CalmMD Code Dark",
        dark: true,
        bg: "#1f1f1f",
        surface: "#181818",
        fg: "#cccccc",
        muted: "#9d9d9d",
        danger: "#f48771",
        link: "#4daafc",
        accent: "#0078d4",
        caret: "#aeafad",
        comment: "#6a9955",
        keyword: "#569cd6",
        string: "#ce9178",
        number: "#b5cea8",
        function: "#dcdcaa",
        type_name: "#4ec9b0",
        property: "#9cdcfe",
        tag: "#569cd6",
    },
    Palette {
        id: "midnight",
        name: "CalmMD Midnight",
        dark: true,
        bg: "#11151f",
        surface: "#0c1018",
        fg: "#c9d1e0",
        muted: "#8c96ab",
        danger: "#f7768e",
        link: "#7aa2f7",
        accent: "#7aa2f7",
        caret: "#c0caf5",
        comment: "#5f6a8f",
        keyword: "#bb9af7",
        string: "#9ece6a",
        number: "#ff9e64",
        function: "#7aa2f7",
        type_name: "#2ac3de",
        property: "#73daca",
        tag: "#f7768e",
    },
    Palette {
        id: "solarized-dark",
        name: "CalmMD Solarized Dark",
        dark: true,
        bg: "#002b36",
        surface: "#01232c",
        fg: "#93a1a1",
        muted: "#839496",
        danger: "#dc322f",
        link: "#268bd2",
        accent: "#b58900",
        caret: "#eee8d5",
        comment: "#657b83",
        keyword: "#859900",
        string: "#2aa198",
        number: "#d33682",
        function: "#268bd2",
        type_name: "#b58900",
        property: "#6c71c4",
        tag: "#dc322f",
    },
    Palette {
        id: "rose",
        name: "CalmMD Rosé",
        dark: false,
        bg: "#fdf9f8",
        surface: "#f7eeec",
        fg: "#3a2f33",
        muted: "#75636a",
        danger: "#b3261e",
        link: "#b02457",
        accent: "#d6336c",
        caret: "#b02457",
        comment: "#a3919a",
        keyword: "#b02457",
        string: "#4f7a4a",
        number: "#b5651d",
        function: "#6a4c9c",
        type_name: "#8a6d1f",
        property: "#3d6a8a",
        tag: "#b02457",
    },
    Palette {
        id: "parchment",
        name: "CalmMD Parchment",
        dark: false,
        bg: "#f3ead7",
        surface: "#eadfc8",
        fg: "#3f3224",
        muted: "#6f5e48",
        danger: "#b3261e",
        link: "#7f3f20",
        accent: "#a0522d",
        caret: "#3f3224",
        comment: "#9c8a70",
        keyword: "#8b3a62",
        string: "#5a7a2e",
        number: "#a0522d",
        function: "#3d5a80",
        type_name: "#7a5c1f",
        property: "#4a6a6a",
        tag: "#8b3a3a",
    },
    Palette {
        id: "fjord",
        name: "CalmMD Fjord",
        dark: true,
        bg: "#2e3440",
        surface: "#292e39",
        fg: "#d8dee9",
        muted: "#a3acbd",
        danger: "#f2817a",
        link: "#88c0d0",
        accent: "#88c0d0",
        caret: "#d8dee9",
        comment: "#7b8497",
        keyword: "#81a1c1",
        string: "#a3be8c",
        number: "#b48ead",
        function: "#88c0d0",
        type_name: "#8fbcbb",
        property: "#d8dee9",
        tag: "#bf616a",
    },
    Palette {
        id: "nightshade",
        name: "CalmMD Nightshade",
        dark: true,
        bg: "#282a36",
        surface: "#21222c",
        fg: "#e9e9e4",
        muted: "#a9abc4",
        danger: "#f2817a",
        link: "#8be9fd",
        accent: "#bd93f9",
        caret: "#f8f8f2",
        comment: "#7c7f9c",
        keyword: "#ff79c6",
        string: "#f1fa8c",
        number: "#bd93f9",
        function: "#50fa7b",
        type_name: "#8be9fd",
        property: "#ffb86c",
        tag: "#ff79c6",
    },
    Palette {
        id: "forest",
        name: "CalmMD Forest",
        dark: true,
        bg: "#1a1f1b",
        surface: "#151916",
        fg: "#d4dcd0",
        muted: "#9aa894",
        danger: "#f2817a",
        link: "#8fc6a8",
        accent: "#7fb069",
        caret: "#d4dcd0",
        comment: "#6e7c69",
        keyword: "#c49bd8",
        string: "#a8cf8e",
        number: "#deb868",
        function: "#8fc6a8",
        type_name: "#e0c477",
        property: "#9ccfd8",
        tag: "#e08a7a",
    },
];

/// Built-in preview themes do not have their own syntax palette. Map them to the closest
/// full CalmMD colour theme instead of falling back to a built-in Mordant theme.
const PREVIEW_THEMES: &[(&str, &str)] = &[
    ("studio", LIGHT_THEME),
    ("typewriter", LIGHT_THEME),
    ("clear", LIGHT_THEME),
    ("github", "CalmMD GitHub Light"),
];

static REGISTERED: OnceLock<()> = OnceLock::new();

/// Registers all CalmMD themes in Mordant's global theme registry. Safe to call repeatedly.
pub fn initialize() {
    REGISTERED.get_or_init(|| {
        for palette in PALETTES {
            register_custom_theme(palette.name, &theme_json(palette))
                .unwrap_or_else(|error| panic!("invalid CalmMD theme {}: {error}", palette.name));
        }
    });
}

/// Resolves a CalmMD theme id to one of the generated Mordant theme names.
///
/// Unknown ids fall back by colour scheme, so a custom CSS theme still gets readable code blocks.
/// Names already known to Mordant are passed through, which keeps explicit theme overrides usable.
pub fn theme_for(theme: Option<&str>, scheme: Option<&str>) -> &'static str {
    initialize();

    if let Some(id) = theme {
        for palette in PALETTES {
            if id.eq_ignore_ascii_case(palette.id) || id.eq_ignore_ascii_case(palette.name) {
                return palette.name;
            }
        }
        for (preview, name) in PREVIEW_THEMES {
            if id.eq_ignore_ascii_case(preview) {
                return name;
            }
        }
    }
    if scheme.is_some_and(|s| s.eq_ignore_ascii_case("dark")) {
        DARK_THEME
    } else {
        LIGHT_THEME
    }
}

/// Resolves an options value to a registered Mordant theme name.
///
/// CalmMD theme ids map to the generated palettes; names already known to Mordant are passed
/// through so an explicit `Dracula` override still works.
pub fn resolve(theme: Option<&str>, scheme: Option<&str>) -> String {
    initialize();
    let mapped = theme_for(theme, scheme);
    if let Some(id) = theme {
        if !is_calm_theme(id) && list_themes().iter().any(|name| name == id) {
            return id.to_owned();
        }
    }
    mapped.to_owned()
}

fn is_calm_theme(id: &str) -> bool {
    PALETTES
        .iter()
        .any(|p| id.eq_ignore_ascii_case(p.id) || id.eq_ignore_ascii_case(p.name))
        || PREVIEW_THEMES
            .iter()
            .any(|(preview, _)| id.eq_ignore_ascii_case(preview))
}

/// Builds a VS Code theme JSON document understood by Mordant's custom theme loader.
fn theme_json(p: &Palette) -> String {
    let rules = json!([
        {
            "scope": ["comment", "punctuation.definition.comment"],
            "settings": { "foreground": p.comment, "fontStyle": "italic" }
        },
        {
            "scope": ["keyword", "storage", "keyword.operator", "variable.language"],
            "settings": { "foreground": p.keyword }
        },
        {
            "scope": ["string", "constant.character", "punctuation.definition.string"],
            "settings": { "foreground": p.string }
        },
        {
            "scope": ["constant.numeric", "constant.language", "support.constant"],
            "settings": { "foreground": p.number }
        },
        {
            "scope": [
                "entity.name.function",
                "support.function",
                "variable.function",
                "entity.name.function.macro",
                "macro"
            ],
            "settings": { "foreground": p.function }
        },
        {
            "scope": [
                "entity.name.type",
                "support.type",
                "entity.name.class",
                "support.class",
                "entity.other.inherited-class"
            ],
            "settings": { "foreground": p.type_name }
        },
        {
            "scope": [
                "variable.other.property",
                "variable.other.member",
                "support.type.property-name",
                "entity.other.attribute-name"
            ],
            "settings": { "foreground": p.property }
        },
        {
            "scope": ["entity.name.tag", "support.class.component"],
            "settings": { "foreground": p.tag }
        },
        {
            "scope": ["meta", "punctuation"],
            "settings": { "foreground": p.muted }
        },
        {
            "scope": ["markup.link", "string.other.link", "constant.other.reference.link"],
            "settings": { "foreground": p.link }
        },
        {
            "scope": ["invalid", "invalid.illegal", "markup.deleted", "punctuation.definition.deleted"],
            "settings": { "foreground": p.danger }
        },
        {
            "scope": ["markup.inserted", "punctuation.definition.inserted"],
            "settings": { "foreground": p.string }
        },
        {
            "scope": ["markup.heading", "markup.bold", "markup.strong"],
            "settings": { "fontStyle": "bold" }
        },
        {
            "scope": ["markup.emphasis", "markup.italic"],
            "settings": { "fontStyle": "italic" }
        },
    ]);
    json!({
        "name": p.name,
        "type": if p.dark { "dark" } else { "light" },
        "colors": {
            "editor.background": p.surface,
            "editor.foreground": p.fg,
            "editorGutter.background": p.bg,
            "editorCursor.background": p.caret,
            "editorLineNumber.foreground": p.muted,
            "editor.selectionBackground": p.accent
        },
        "tokenColors": rules,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mordant::highlighter::{highlight_code, theme_background, HighlightingMode};

    const TOKENS_CSS: &str = include_str!("../../../../../src/styles/tokens.css");

    fn css_block(selector: &str) -> &'static str {
        let start = TOKENS_CSS
            .find(selector)
            .unwrap_or_else(|| panic!("missing CSS selector {selector}"));
        let end = TOKENS_CSS[start..]
            .find("\n}")
            .map(|offset| start + offset)
            .unwrap_or(TOKENS_CSS.len());
        &TOKENS_CSS[start..end]
    }

    fn css_value<'a>(block: &'a str, variable: &str) -> Option<&'a str> {
        let needle = format!("  {variable}: ");
        let line = block.lines().find(|line| line.starts_with(&needle))?;
        Some(line.trim_start_matches(&needle).trim_end_matches(';'))
    }

    fn selector_for(p: &Palette) -> String {
        if p.id == "snow" {
            ":root {".to_owned()
        } else if p.id == "ink" {
            ":root[data-theme='dark'] {".to_owned()
        } else if p.dark {
            format!(":root[data-theme='dark'][data-color-theme='{}'] {{", p.id)
        } else {
            format!(":root[data-color-theme='{}'] {{", p.id)
        }
    }

    #[test]
    fn palette_colours_match_tokens_css() {
        for p in PALETTES {
            let block = css_block(&selector_for(p));
            let values = [
                ("--bg", p.bg),
                ("--surface", p.surface),
                ("--fg", p.fg),
                ("--fg-muted", p.muted),
                ("--syntax-comment", p.comment),
                ("--syntax-keyword", p.keyword),
                ("--syntax-string", p.string),
                ("--syntax-number", p.number),
                ("--syntax-function", p.function),
                ("--syntax-type", p.type_name),
                ("--syntax-property", p.property),
                ("--syntax-tag", p.tag),
            ];
            for (variable, expected) in values {
                if let Some(actual) = css_value(block, variable) {
                    assert_eq!(actual, expected, "{} in {}", variable, p.id);
                }
            }
        }
    }

    #[test]
    fn registers_one_theme_for_every_calm_color_theme() {
        initialize();
        let registered = list_themes();
        for palette in PALETTES {
            assert!(
                registered.iter().any(|name| name == palette.name),
                "{} is not registered",
                palette.name
            );
            assert_eq!(theme_background(palette.name).as_deref(), Some(palette.surface));
        }
    }

    #[test]
    fn maps_palette_ids_and_preview_themes() {
        initialize();
        assert_eq!(theme_for(Some("snow"), None), LIGHT_THEME);
        assert_eq!(theme_for(Some("PAPER"), None), "CalmMD Paper");
        assert_eq!(theme_for(Some("github-dark"), None), "CalmMD GitHub Dark");
        assert_eq!(theme_for(Some("studio"), None), LIGHT_THEME);
        assert_eq!(theme_for(Some("github"), None), "CalmMD GitHub Light");
        assert_eq!(theme_for(None, Some("light")), LIGHT_THEME);
        assert_eq!(theme_for(None, Some("dark")), DARK_THEME);
        assert_eq!(theme_for(Some("user:Custom.css"), Some("dark")), DARK_THEME);
        assert_eq!(resolve(Some("InspiredGitHub"), None), "InspiredGitHub");
        assert_eq!(resolve(Some("github-dark"), None), "CalmMD GitHub Dark");
    }

    #[test]
    fn highlighting_uses_calm_token_colours() {
        initialize();
        let code = "// note\nlet answer = 42; // comment\n";
        let html = highlight_code("rust", code, "CalmMD Snow", &HighlightingMode::Attribute);
        assert!(html.contains("#7d838e"), "comment colour missing: {html}");
        assert!(html.contains("#7a3e9d"), "keyword colour missing: {html}");
        assert!(html.contains("#a3561a"), "number colour missing: {html}");
    }
}
