//! Deterministic final assembly for standalone HTML exports.
//!
//! This mirrors `src/export/build.ts::buildDocument`. The webview still prepares the body (its
//! Mermaid and CSS asset loaders live there), but the final document shell and print CSS are built
//! here so MCP exports no longer depend on webview string templating.

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssemblePdfOptions {
    pub page_size: String,
    pub margins: String,
    pub header: bool,
    pub page_numbers: bool,
    pub break_before_h1: bool,
    pub template: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssembleRequest {
    pub body: String,
    pub title: String,
    pub theme: String,
    pub user_css: Option<String>,
    pub base_css: String,
    pub font_css: String,
    pub katex_css: Option<String>,
    pub paged_js: Option<String>,
    pub pdf: Option<AssemblePdfOptions>,
}

const EXPORT_CSS: &str = r#"
html, body { margin: 0; background: var(--preview-bg, var(--bg)); }
body { -webkit-font-smoothing: antialiased; text-rendering: optimizeLegibility; }
.md-body { padding: 56px 32px 96px; }
.md-body .page-break { border: 0; margin: 0; }
.md-body .page-break::after { display: none; }
.md-body nav.toc .toc-dots { display: none; }
.md-body .task-checkbox { cursor: default; }
.md-body .front-matter { display: none; }
.md-body .cover { margin: 0 0 3em; padding: 2.5em 0 2em; border-bottom: 1px solid var(--hairline-strong); }
.md-body .cover-title { margin: 0 0 0.3em; font-family: var(--md-heading-font); font-size: 2.6em; font-weight: 700; line-height: 1.15; color: var(--fg-strong); letter-spacing: -0.015em; }
.md-body .cover-subtitle { margin: 0 0 1.6em; font-size: 1.3em; color: var(--fg-muted); }
.md-body .cover-meta p { margin: 0.2em 0; color: var(--fg-muted); font-family: var(--font-ui); font-size: 0.9em; }
.md-body .embed-link { display: block; padding: 10px 14px; border: 1px solid var(--hairline-strong); border-radius: 8px; color: var(--fg-strong); text-decoration: none; margin: 0 0 var(--md-gap); }
.md-body .embed-link span { display: block; font: 9pt var(--font-ui); color: var(--fg-faint); overflow-wrap: anywhere; }
"#;

const TEMPLATE_CSS_STANDARD: &str = "";
const TEMPLATE_CSS_REPORT: &str = r#"
.md-body h1 { padding: 0.15em 0 0.15em 0.6em; border-left: 5px solid var(--accent); }
.md-body h2 { padding-bottom: 0.25em; border-bottom: 2px solid color-mix(in srgb, var(--accent) 60%, transparent); }
.md-body table { width: 100%; }
.md-body th { background: color-mix(in srgb, var(--accent) 14%, transparent); }
.md-body tbody tr:nth-child(2n) { background: var(--surface); }
.md-body .cover-title { border-left: 8px solid var(--accent); padding-left: 0.4em; }
"#;
const TEMPLATE_CSS_ACADEMIC: &str = r#"
.md-body { --md-font: var(--font-serif); --md-heading-font: var(--font-serif); --md-leading: 1.55; counter-reset: h2; }
.md-body p, .md-body li { text-align: justify; hyphens: auto; }
.md-body > h1:first-of-type { text-align: center; font-size: 1.8em; margin-bottom: 1.2em; }
.md-body h2 { counter-increment: h2; counter-reset: h3; font-size: 1.25em; }
.md-body h2::before { content: counter(h2) ". "; }
.md-body h3 { counter-increment: h3; font-size: 1.08em; }
.md-body h3::before { content: counter(h2) "." counter(h3) " "; }
.md-body blockquote { font-style: normal; border-left-width: 2px; }
.md-body .cover { text-align: center; }
"#;
const TEMPLATE_CSS_MINIMAL: &str = r#"
.md-body { --md-font: var(--font-ui); --md-heading-font: var(--font-ui); --md-leading: 1.6; }
.md-body h1, .md-body h2, .md-body h3 { font-weight: 600; letter-spacing: -0.01em; }
.md-body h2 { border: 0; }
.md-body hr { width: 100%; background: var(--hairline); }
.md-body pre, .md-body :not(pre) > code { border: 0; }
"#;

fn template_css(template: &str) -> &'static str {
    match template {
        "report" => TEMPLATE_CSS_REPORT,
        "academic" => TEMPLATE_CSS_ACADEMIC,
        "minimal" => TEMPLATE_CSS_MINIMAL,
        _ => TEMPLATE_CSS_STANDARD,
    }
}

fn margins(value: &str) -> &'static str {
    match value {
        "narrow" => "14mm 14mm 16mm",
        "wide" => "30mm 28mm 32mm",
        _ => "22mm 20mm 24mm",
    }
}

fn print_size(theme: &str) -> &'static str {
    match theme {
        "paper" => "11pt",
        "typewriter" => "10pt",
        "studio" | "github" | "clear" => "10.5pt",
        _ => "10.5pt",
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn css_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\r' | '\n' => escaped.push(' '),
            _ => escaped.push(c),
        }
    }
    format!("\"{escaped}\"")
}

/// Mirrors JS `String.replace(/needle/gi, replacement)`.
fn replace_case_insensitive(text: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let pat: Vec<char> = needle.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if i + pat.len() <= chars.len()
            && chars[i..i + pat.len()]
                .iter()
                .zip(pat.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            out.push_str(replacement);
            i += pat.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn safe_css(css: &str) -> String {
    replace_case_insensitive(css, "</style", "<\\/style")
}

fn safe_script(script: &str) -> String {
    replace_case_insensitive(script, "</script", "<\\/script")
}

fn pdf_css(opts: &AssembleRequest, pdf: &AssemblePdfOptions) -> String {
    let box_css = "font-family: var(--font-ui); font-size: 8.5pt; color: #8a8f99;";
    let title = if pdf.header && !opts.title.is_empty() {
        Some(css_string(&opts.title))
    } else {
        None
    };
    let header = if pdf.template == "minimal" || title.is_none() {
        String::new()
    } else if pdf.template == "report" {
        format!(
            "@top-left {{ content: {}; {} }} @top-right {{ content: counter(page); {} }}",
            title.as_deref().unwrap_or(""), box_css, box_css
        )
    } else {
        format!("@top-center {{ content: {}; {} }}", title.as_deref().unwrap_or(""), box_css)
    };
    let footer = if !pdf.page_numbers {
        String::new()
    } else {
        match pdf.template.as_str() {
            "report" => format!(
                "@bottom-right {{ content: \"Page \" counter(page) \" of \" counter(pages); {} }}",
                box_css
            ),
            "minimal" => format!("@bottom-right {{ content: counter(page); {} }}", box_css),
            _ => format!(
                "@bottom-center {{ content: counter(page) \" / \" counter(pages); {} }}",
                box_css
            ),
        }
    };
    let break_css = if pdf.break_before_h1 {
        ".md-body h1 { break-before: page; } .md-body > h1:first-child, .md-body nav.toc + h1 { break-before: auto; }"
    } else {
        ""
    };
    format!(
        r#"
@page {{ size: {}; margin: {}; {} {} }}
@page :first {{ @top-center {{ content: none; }} @top-left {{ content: none; }} @top-right {{ content: none; }} }}
* {{ -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
html, body {{ background: var(--preview-bg, #fff); }}
.md-body {{ max-width: none; margin: 0; padding: 0; orphans: 3; widows: 3; }}
.md-body h1, .md-body h2, .md-body h3, .md-body h4, .md-body h5, .md-body h6 {{ break-after: avoid; }}
.md-body img, .md-body tr, .md-body blockquote, .md-body figure, .md-body .katex-display, .md-body .mermaid {{ break-inside: avoid; }}
.md-body pre, .md-body pre code {{ white-space: pre-wrap; overflow-wrap: anywhere; }}
.md-body pre {{ overflow: visible; }}
.md-body pre[data-lang]::after {{ display: none; }}
.md-body table {{ display: table; width: 100%; max-width: 100%; }}
.md-body img {{ max-height: 230mm; }}
.md-body a {{ text-decoration: none; }}
.md-body .page-break {{ break-after: page; }}
.md-body nav.toc {{ break-after: page; }}
.md-body .cover {{ page: cover; break-after: page; border: 0; margin: 0; padding: 62mm 0 0; }}
@page cover {{ @top-center {{ content: none; }} @top-left {{ content: none; }} @top-right {{ content: none; }} @bottom-center {{ content: none; }} @bottom-right {{ content: none; }} }}
.md-body nav.toc a {{ display: flex; align-items: baseline; gap: 0.4em; }}
.md-body nav.toc .toc-dots {{ display: block; flex: 1; border-bottom: 1px dotted var(--fg-faint); }}
.md-body nav.toc a::after {{ content: target-counter(attr(href url), page); font-variant-numeric: tabular-nums; }}
{}
"#,
        pdf.page_size,
        margins(&pdf.margins),
        header,
        footer,
        break_css
    )
}

pub fn assemble(opts: &AssembleRequest) -> String {
    let has_math = opts.body.contains("class=\"katex");
    let mut theme_css = vec![
        opts.font_css.clone(),
        if has_math { opts.katex_css.clone().unwrap_or_default() } else { String::new() },
        opts.base_css.clone(),
        EXPORT_CSS.to_owned(),
    ];
    if let Some(pdf) = &opts.pdf {
        theme_css.push(format!(".md-body {{ --md-size: {}; }}", print_size(&opts.theme)));
        theme_css.push(template_css(&pdf.template).to_owned());
    }
    theme_css.push(opts.user_css.clone().unwrap_or_default());
    let theme_css = theme_css.join("\n");

    let styles = if let Some(pdf) = &opts.pdf {
        format!(
            "<style data-pagedjs-ignore>\n{}\n</style>\n<style>\n{}\n</style>",
            safe_css(&theme_css),
            safe_css(&pdf_css(opts, pdf))
        )
    } else {
        format!("<style>\n{}\n</style>", safe_css(&theme_css))
    };

    let paged = if let (Some(pdf), Some(paged_js)) = (&opts.pdf, &opts.paged_js) {
        let _ = pdf;
        "<script>window.PagedConfig = { auto: true, after: function () { document.fonts.ready.then(function () { window.__calmmdDone = true; }); } };</script>\n<script>".to_owned()
            + &safe_script(paged_js)
            + "</script>"
    } else if opts.pdf.is_some() {
        "<script>window.addEventListener('load', function () { document.fonts.ready.then(function () { window.__calmmdDone = true; }); });</script>".to_owned()
    } else {
        String::new()
    };

    format!(
        "<!doctype html>\n<html lang=\"en\" data-theme=\"light\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"generator\" content=\"CalmMD\">\n<title>{}</title>\n{}\n{}\n</head>\n<body>\n<article class=\"md-body\" data-preview-theme=\"{}\">\n{}\n</article>\n</body>\n</html>\n",
        escape_html(&opts.title),
        styles,
        paged,
        escape_html(&opts.theme),
        opts.body
    )
}

pub fn assemble_from_value(payload: Value) -> Result<Value, String> {
    let request: AssembleRequest = serde_json::from_value(payload).map_err(|e| e.to_string())?;
    Ok(json!({ "html": assemble(&request) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> AssembleRequest {
        AssembleRequest {
            body: "<h1 id=\"a\">Hello</h1>".into(),
            title: "A & B".into(),
            theme: "paper".into(),
            user_css: Some(".x{}".into()),
            base_css: "body{}".into(),
            font_css: "@font-face{}".into(),
            katex_css: None,
            paged_js: None,
            pdf: None,
        }
    }

    #[test]
    fn assembles_a_standalone_document() {
        let html = assemble(&request());
        assert!(html.starts_with("<!doctype html>\n<html lang=\"en\" data-theme=\"light\">"));
        assert!(html.contains("<title>A &amp; B</title>"));
        assert!(html.contains("<article class=\"md-body\" data-preview-theme=\"paper\">"));
        assert!(html.contains("<h1 id=\"a\">Hello</h1>"));
    }

    #[test]
    fn uses_paged_media_only_for_pdf() {
        let mut opts = request();
        assert!(!assemble(&opts).contains("@page"));
        opts.pdf = Some(AssemblePdfOptions {
            page_size: "A4".into(),
            margins: "normal".into(),
            header: true,
            page_numbers: true,
            break_before_h1: false,
            template: "report".into(),
        });
        let html = assemble(&opts);
        assert!(html.contains("data-pagedjs-ignore"));
        assert!(html.contains("@page { size: A4; margin: 22mm 20mm 24mm;"));
        assert!(html.contains("window.__calmmdDone"));
    }

    #[test]
    fn escapes_style_and_script_closing_tags() {
        let mut opts = request();
        opts.user_css = Some("</style><b>".into());
        let html = assemble(&opts);
        assert!(html.contains("<\\/style><b>"));
        opts.pdf = Some(AssemblePdfOptions {
            page_size: "A4".into(),
            margins: "normal".into(),
            header: false,
            page_numbers: false,
            break_before_h1: false,
            template: "standard".into(),
        });
        opts.paged_js = Some("</script>".into());
        let html = assemble(&opts);
        assert!(html.contains("<\\/script>"));
    }
}
