# Third-party licenses

Runtime dependencies shipped with CalmMD. Only permissive licenses are allowed.

| Package | License | Use |
|---|---|---|
| Tauri 2 (`tauri`, `@tauri-apps/api`, `tauri-plugin-dialog`) | MIT / Apache-2.0 | Desktop shell |
| CodeMirror 6 (`@codemirror/*`, `@lezer/*`) | MIT | Editor |
| Solid (`solid-js`) | MIT | UI layer |
| markdown-it, markdown-it-footnote | MIT | Markdown rendering |
| DOMPurify | Apache-2.0 (dual-licensed with MPL-2.0; used under Apache-2.0) | HTML sanitising |
| `tauri-plugin-opener`, `@tauri-apps/plugin-opener` | MIT / Apache-2.0 | Opening links in the browser |
| `tauri-plugin-clipboard-manager`, `@tauri-apps/plugin-clipboard-manager` | MIT / Apache-2.0 | Clipboard for context-menu Cut/Copy/Paste |
| `@codemirror/autocomplete` | MIT | Slash-command menu |
| `tauri-plugin-single-instance` | MIT / Apache-2.0 | Opening files in the running app |
| serde, serde_json | MIT / Apache-2.0 | Rust serialisation |
| sha2 | MIT OR Apache-2.0 | Content hashes for Rust-owned file history |
| Mordant (`mordant`) and its highlighting dependencies (syntect, syntect-assets, jsonc-parser) | MIT / MIT / Apache-2.0 | Rust Markdown parsing, rendering, math and fenced-code highlighting (feature-flagged) |
| `katex-rs` | MIT | Rust-side KaTeX math rendering in Mordant (feature-flagged) |
| Takumi (`takumi-core`, `takumi-pdf`, `@takumi-rs/helpers`) | MIT OR Apache-2.0 | Default native Rust PDF engine on desktop and local WASM fallback in the browser; `VITE_CALMMD_PDF_ENGINE=browser` selects the older engine |
| zspell (`zspell`) | Apache-2.0 | Native Rust spellchecking (feature-flagged) |
| en_US / en_GB Hunspell dictionaries (`wooorm/dictionaries`) | MIT AND BSD | Bundled spellcheck dictionaries and personal words; full texts beside each dictionary |
| Generated word lists (`wordfreq` 3.1.1, Apache-2.0) | Apache-2.0 | German/French/Spanish are bundled; Italian, Portuguese, Dutch/Flemish, Finnish, Norwegian, Danish, Polish, Chinese, Japanese and Korean are downloaded on request from the repository assets; full text beside each dictionary |
| Optional Hunspell dictionaries (`wooorm/dictionaries`) | MIT / BSD-3-Clause | Downloaded on request into app data; licences are saved with each dictionary |
| reqwest | MIT / Apache-2.0 | HTTPS downloads for optional spellcheck dictionaries |
| `rmcp` | Apache-2.0 | Embedded local MCP server (`2026-07-28` spec) |
| axum, tokio | MIT | Loopback HTTP server for MCP |
| uuid, schemars | MIT / Apache-2.0 | MCP connection tokens and tool schemas |
| trash | MIT | Moving files to the OS trash |
| Paged.js (`pagedjs`) | MIT | PDF pagination (inlined into PDF exports) |
| KaTeX (`katex`) | MIT | Math rendering (fonts: SIL OFL 1.1 / MIT as shipped by KaTeX) |
| Mermaid (`mermaid`) and its bundled dependencies (d3, dagre-d3-es, cytoscape, khroma, …) | MIT / BSD-3 / ISC | Diagrams (lazy-loaded) |
| markdown-it-mark, markdown-it-sub, markdown-it-sup | MIT | `==highlight==`, `~sub~`, `^sup^` |
| base64 | MIT / Apache-2.0 | Embedding images, decoding PDFs |
| tungstenite | MIT / Apache-2.0 | DevTools-protocol connection to the PDF engine |
| Excalidraw (`@excalidraw/excalidraw`) and its bundled dependencies (roughjs, perfect-freehand, Radix UI, jotai, pako, harfbuzzjs, …) | MIT / ISC / BSD-3 / Apache-2.0 / Zlib / 0BSD / CC0 | Drawings (lazy-loaded) |
| Turndown, turndown-plugin-gfm | MIT | Pasting formatted text as Markdown |
| React, React DOM (`react`, `react-dom`) | MIT | Hosts the Excalidraw editor (lazy-loaded) |
| Excalidraw fonts: Excalifont, Virgil, Nunito, Lilita One, Cascadia Code, Liberation Sans, Assistant | SIL OFL 1.1 | Text in drawings |
| Excalidraw font: Comic Shanns | MIT | Text in drawings |
| Inter (`@fontsource-variable/inter`) | SIL OFL 1.1 | UI and sans font |
| Literata (`@fontsource-variable/literata`) | SIL OFL 1.1 | Serif reading font |
| JetBrains Mono (`@fontsource-variable/jetbrains-mono`) | SIL OFL 1.1 | Code and source font |

**Excluded on purpose:** `elkjs` (EPL-2.0 OR GPL-3.0), which Mermaid uses only for the opt-in
`layout: elk` mode. It's aliased to a stub (`src/render/elk-stub.ts`) so no non-permissive code ships.

Dev-only tooling (Vite, Vitest, TypeScript, jsdom, tempfile) is not distributed.

Phase 7 cleanup changed no runtime dependency: it only narrowed Tauri capabilities to the event,
menu and window APIs actually used by the webview and removed unused internal Rust helpers.

**GPUI Kit shell preview (not the default binary yet):** `gpui-kit` 0.10.0 (Apache-2.0, re-exports a
pinned GPUI snapshot, with its `syntect` highlighting feature enabled) and `rfd` (MIT / Apache-2.0)
for native file dialogs and `fs2` (MIT) for the single-instance lock. `syntect` and
`syntect-assets` are already used by the Rust Markdown path. The Rust core also vendors the existing
Fontsource (SIL OFL 1.1), KaTeX (MIT/OFL) and Paged.js (MIT) export assets so MCP can build an
offline standalone document without npm at runtime.
GPUI Kit's documentation
prose is CC BY 4.0, so this project summarises it rather than copying it into CalmMD docs. Optional
`gpui-webview` (MIT / Apache-2.0 through Wry) is still planned for browser-only fallbacks only.
Each dependency's transitive licences must be reviewed again before the GPUI binary becomes the
default.
