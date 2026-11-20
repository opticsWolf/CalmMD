//! Offline export assets for the Rust core.
//!
//! The Tauri webview used to load these through Vite. Keeping local copies here lets MCP produce
//! a self-contained document without a browser process or network access.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

const BASE_CSS: &str = include_str!("../../../assets/export/tokens.css");
const PREVIEW_CSS: &str = include_str!("../../../assets/export/preview.css");
const KATEX_CSS: &str = include_str!("../../../assets/export/katex/katex.min.css");
const PAGED_JS: &str = include_str!("../../../assets/export/paged.polyfill.min.js");

macro_rules! font_bytes {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/export/katex/fonts/", $name))
    };
}

fn katex_font(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "KaTeX_AMS-Regular.woff2" => font_bytes!("KaTeX_AMS-Regular.woff2"),
        "KaTeX_Caligraphic-Bold.woff2" => font_bytes!("KaTeX_Caligraphic-Bold.woff2"),
        "KaTeX_Caligraphic-Regular.woff2" => font_bytes!("KaTeX_Caligraphic-Regular.woff2"),
        "KaTeX_Fraktur-Bold.woff2" => font_bytes!("KaTeX_Fraktur-Bold.woff2"),
        "KaTeX_Fraktur-Regular.woff2" => font_bytes!("KaTeX_Fraktur-Regular.woff2"),
        "KaTeX_Main-Bold.woff2" => font_bytes!("KaTeX_Main-Bold.woff2"),
        "KaTeX_Main-BoldItalic.woff2" => font_bytes!("KaTeX_Main-BoldItalic.woff2"),
        "KaTeX_Main-Italic.woff2" => font_bytes!("KaTeX_Main-Italic.woff2"),
        "KaTeX_Main-Regular.woff2" => font_bytes!("KaTeX_Main-Regular.woff2"),
        "KaTeX_Math-BoldItalic.woff2" => font_bytes!("KaTeX_Math-BoldItalic.woff2"),
        "KaTeX_Math-Italic.woff2" => font_bytes!("KaTeX_Math-Italic.woff2"),
        "KaTeX_SansSerif-Bold.woff2" => font_bytes!("KaTeX_SansSerif-Bold.woff2"),
        "KaTeX_SansSerif-Italic.woff2" => font_bytes!("KaTeX_SansSerif-Italic.woff2"),
        "KaTeX_SansSerif-Regular.woff2" => font_bytes!("KaTeX_SansSerif-Regular.woff2"),
        "KaTeX_Script-Regular.woff2" => font_bytes!("KaTeX_Script-Regular.woff2"),
        "KaTeX_Size1-Regular.woff2" => font_bytes!("KaTeX_Size1-Regular.woff2"),
        "KaTeX_Size2-Regular.woff2" => font_bytes!("KaTeX_Size2-Regular.woff2"),
        "KaTeX_Size3-Regular.woff2" => font_bytes!("KaTeX_Size3-Regular.woff2"),
        "KaTeX_Size4-Regular.woff2" => font_bytes!("KaTeX_Size4-Regular.woff2"),
        "KaTeX_Typewriter-Regular.woff2" => font_bytes!("KaTeX_Typewriter-Regular.woff2"),
        _ => return None,
    })
}

/// CSS assets required by `assemble`, with all fonts inlined as data URLs.
#[derive(Clone, Debug)]
pub struct ExportAssets {
    pub base_css: String,
    pub font_css: String,
    pub katex_css: Option<String>,
    pub paged_js: Option<String>,
}

impl ExportAssets {
    pub fn load(with_paged: bool, with_math: bool) -> Self {
        Self {
            base_css: format!("{BASE_CSS}\n{PREVIEW_CSS}"),
            font_css: font_css(),
            katex_css: with_math.then(inline_katex_fonts),
            paged_js: with_paged.then(|| PAGED_JS.to_owned()),
        }
    }
}

fn face(family: &str, bytes: &[u8]) -> String {
    format!(
        "@font-face {{ font-family: '{family}'; font-style: normal; font-display: block; font-weight: 100 900; src: url(data:font/woff2;base64,{}) format('woff2'); }}",
        BASE64.encode(bytes)
    )
}

fn font_css() -> String {
    [
        face("Inter Variable", include_bytes!("../../../assets/export/fonts/inter.woff2")),
        face("Literata Variable", include_bytes!("../../../assets/export/fonts/literata.woff2")),
        face("JetBrains Mono Variable", include_bytes!("../../../assets/export/fonts/jetbrains-mono.woff2")),
    ]
    .join("\n")
}

/// Mirrors `loadKatexCss`: keep only the woff2 source and inline it. The CSS file always lists
/// woff2 first, followed by woff/truetype fallbacks that are not vendored here.
fn inline_katex_fonts() -> String {
    const MARKER: &str = "url(fonts/";
    let mut out = String::with_capacity(KATEX_CSS.len() + 64 * 1024);
    let mut rest = KATEX_CSS;
    while let Some(offset) = rest.find(MARKER) {
        out.push_str(&rest[..offset]);
        let after = &rest[offset..];
        let Some(end) = after.find(')') else {
            out.push_str(after);
            return out;
        };
        let name = &after[MARKER.len()..end];
        let mut tail = &after[end + 1..];
        if let Some(format) = tail.strip_prefix(" format(") {
            if let Some(format_end) = format.find(')') {
                tail = &format[format_end + 1..];
            }
        }
        if let Some(bytes) = katex_font(name) {
            out.push_str("url(data:font/woff2;base64,");
            out.push_str(&BASE64.encode(bytes));
            out.push(')');
        } else {
            // Remove the comma that preceded the skipped fallback source.
            if out.ends_with(',') {
                out.pop();
            }
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inlines_only_woff2_katex_fonts() {
        let css = inline_katex_fonts();
        assert!(css.contains("url(data:font/woff2;base64,"));
        assert!(!css.contains("url(fonts/"));
        assert!(!css.contains("format(\"woff\")"));
        assert!(!css.contains("format(\"truetype\")"));
    }

    #[test]
    fn load_omits_heavy_assets_unless_requested() {
        let assets = ExportAssets::load(false, false);
        assert!(assets.katex_css.is_none());
        assert!(assets.paged_js.is_none());
        assert!(assets.font_css.contains("Inter Variable"));
        assert!(assets.base_css.contains("--fg-strong"));
    }
}
