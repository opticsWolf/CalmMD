# GPUI Kit export and platform gate: phases 14–15

## Export without the bridge

- [x] `calmmd-core` owns vendored `tokens.css`, `preview.css`, variable fonts, KaTeX CSS/fonts and
      the Paged.js polyfill under `crates/calmmd-core/assets/export`.
- [x] `core/export/body.rs` renders Markdown, strips editor-only attributes, embeds local images,
      flattens wiki links, converts video cards to print links, and builds the TOC/cover.
- [x] `calmmd_export_html` and `calmmd_export_pdf` are now served directly by Rust; no
      `calmmd://mcp/request` message is sent for exports.
- [x] Takumi is the default PDF backend. Paged.js assets remain inlined for explicit browser
      comparison only.
- [x] Core tests assert the Markdown/extension snapshots, standalone HTML assembly and a real PDF
      signature without a UI process.
- [ ] Final visual comparison against the Takumi/Paged.js review remains part of the cutover gate;
      this phase did not change the print CSS or the accepted Takumi adaptations.

## Platform gate

- [x] File menu, Edit menu and View menu are installed for both native and in-window menu paths.
- [x] Right-click uses GPUI Kit `NativeMenu` with a Linux drawn-menu fallback.
- [x] Folder/save dialogs, clipboard, external URL allow-list and reveal-in-folder go through the
      `Platform` trait.
- [x] External file drops and OS launch paths open through the core workspace service.
- [x] Single-instance locking queues launch paths for the running process.
- [x] Platform limits are documented in `docs/PLATFORM-LIMITS.md`.

Manual checks still required on each target OS at cutover:

- [ ] Native menu bar and right-click menu placement.
- [ ] Double-clicking a Markdown file in the shell while CalmMD is already running.
- [ ] Drag-and-drop from Explorer/Finder/files.
- [ ] Reveal-in-folder and external-link behaviour.
