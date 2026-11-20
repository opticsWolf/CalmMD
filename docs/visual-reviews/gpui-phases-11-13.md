# GPUI Kit preview review: phases 11–13

This review records the native GPUI preview introduced by phases 11–13. The
Tauri webview remains the shipped UI until the cutover gate, so this is a
review of the preview shell, not a replacement for the Takumi/Paged.js export
review.

## Editor gate

- [x] Source pane uses GPUI Kit `Editor` with Markdown syntax highlighting,
      line numbers and folding.
- [x] Every user edit reports into `WorkspaceState`, which hands autosave to
      `calmmd-core`; status and conflict actions come from core autosave events.
- [x] zspell UTF-16 ranges are converted to UTF-8 byte ranges and painted as
      wavy spell underlines. Search matches use a separate decoration layer.
- [x] Link clicks, `[[wiki links]]`, sidebar search, undo, redo and history
      route through core services.
- [x] The frozen 500-line anchor fixture remains asserted by
      `calmmd-core/src/core/markdown/render.rs`; the GPUI shell consumes the
      same text and core services.

## Preview gate

- [x] Preview uses `TextView::markdown`, not browser HTML or DOMPurify.
- [x] Front matter uses Kit's front-matter plugin; callouts use native
      `block_surface` containers.
- [x] Light/dark palettes are derived from `src/styles/tokens.css` and toggle
      with `Ctrl/Cmd+Alt+T`.
- [x] Extension corpus items (tasks, tables, footnotes, code fences, wiki
      links, callouts, math, page breaks) remain readable in native text.

## Media gate

- [x] Local and data-URL images use GPUI's native text-view image handling.
- [x] YouTube/Vimeo/local video links render as click-to-open cards; no
      third-party request happens while merely viewing a note.
- [x] Page-break markers render as a labelled rule in the preview.
- [x] `$...$` and `$$...$$` render as readable native math text for common
      Greek letters, powers and indices.
- [x] Mermaid fences render as a labelled native source card. Graphical Mermaid
      parity is deferred to the diagram gate; `gpui-webview` remains an
      explicit, optional fallback and is not installed in the default shell.

## Manual checks still required at cutover

- [ ] Compare light/dark screenshots with the Tauri renderer on Windows, macOS
      and Linux.
- [ ] Confirm font fallback for Literata/JetBrains Mono substitutes on each
      platform.
- [ ] Decide whether native Mermaid source cards are sufficient or whether a
      local SVG renderer is needed before removing the Tauri fallback.
