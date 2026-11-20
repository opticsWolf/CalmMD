# CalmMD architecture

How CalmMD is put together, and the rules that keep it working. Read [SPEC.md](../SPEC.md) first for the
product decisions and design principles, and [CONTRIBUTING.md](../CONTRIBUTING.md) for how to work on it.
The Rust service layer (Mordant, zspell, Takumi and ikat) is described in
[`RUST-CORE-PLAN.md`](RUST-CORE-PLAN.md). That document also tracks the active GPUI Kit shell
migration. The Tauri shell is still the shipped desktop app; `apps/calmmd-gpui` is the in-progress
native shell preview.

## Commands

| What | Command |
|---|---|
| Install | `npm install` (needs Node 20+, and Rust stable + platform deps for the desktop app) |
| Browser dev (no Rust needed) | `npm run dev` → http://localhost:1420 (localStorage sandbox platform) |
| Desktop dev | `npm run app:dev` |
| Unit tests (TS) | `npm test` |
| Unit tests (Rust) | `cargo test --workspace` |
| GPUI shell preview | `cargo run -p calmmd-gpui` (needs GPUI Kit platform dependencies) |
| Typecheck | `npm run typecheck` |
| Production build | `npm run app:build` |
| Regenerate icons | `npx tauri icon assets/brand/calmmd-mark.svg` (then delete `src-tauri/icons/{android,ios}`) |

On Windows, Rust needs the MSVC build tools (`winget install Microsoft.VisualStudio.2022.BuildTools`
with the VC tools workload). On Linux, see the Tauri prerequisites (webkit2gtk-4.1, etc.).

## Rust workspace

```text
crates/calmmd-core/   UI-independent services, DTOs, MCP server; no Tauri or GPUI imports
src-tauri/            legacy Tauri shell and command adapters (still the shipped binary)
apps/calmmd-gpui/     GPUI Kit shell preview: Dock, Actions, Entity adapters, Platform seams
```

`AppContext` gives each shell the same data directory, event sink and window registry. Core events
are emitted through that context, so Tauri uses `emit_to` while GPUI forwards them into an Entity
that calls `cx.notify()`. MCP calls the same `WorkspaceRegistry` directly in both shells.

## Architecture

```
src/
  brand.ts            product name and tagline (the only place in the frontend)
  main.tsx, App.tsx   bootstrap and app shell (layout, global shortcuts, wiring)
  app/documents.ts    document manager: tabs, open/save/save-as, autosave targets,
                      drafts, external-change and conflict handling (framework-light, tested)
  app/settings.ts     persisted settings (view mode, split ratio, sync, themes) in kv "settings"
  app/session.ts      per-window session (tabs + cursor + top line, root, expanded, sidebar, viewMode)
  app/spaces.ts       Spaces store: kv "space/<id>" = { name, lastOpened, session }
  app/explorer.ts     explorer state: root, lazy listings, favorites (kv "favorites"), create/rename/trash
  core/outline.ts     heading extraction (line scanner: fences, setext, front matter) for the outline
  core/markdownService.ts  typed client for the feature-flagged Rust Markdown commands
  core/rustFlags.ts   VITE_CALMMD_* / localStorage switches for the Rust services and PDF engine
  markdown/types.ts   DTOs mirrored from crates/calmmd-core/src/core/dto.rs
  app/preview.ts      preview controller: debounced render, DOM patch, links, task toggles
  render/markdown.ts  markdown-it + footnotes + task lists + heading ids + source-line attrs,
                      sanitised with DOMPurify. render/highlight.ts reuses the editor's Lezer grammars
  render/rustMarkdown.ts  default Mordant-backed preview renderer; keeps webview-only images,
                      Mermaid, embeds and page breaks, then sanitises with DOMPurify
  render/patch.ts     reuses unchanged top-level blocks between renders (keeps images and embeds stable)
  editor/live/        live (hybrid) mode
    analyze.ts        PURE: syntax tree + selection → LiveItems (hide, mark, line, widgets). Tested.
    decorations.ts    LiveItems → CM decorations (ViewPlugin, visible ranges); tables as block widgets
                      (StateField); mousedown handlers (task toggle, Mod+click link)
    widgets.ts        bullet, task, hr, image (+ companion image while editing), table
    tableWidget.ts    editable tables: cells are contenteditable (raw Markdown while focused, rendered
                      after), Tab/Enter move and add rows, changes written back with formatTable; the
                      widget updates its own source so redraws from its edits keep focus; right-click
                      actions via the element's tableActions (App onEditorContextMenu)
    config.ts         facet with app hooks (image resolution, table rendering, link opening)
  styles/editor.css   centred column (--pad via container units) and live typography
  app/links.ts        one link policy for preview and live (anchors, web, relative .md, else ignore)
  app/commands.ts     CommandRegistry: the single source for palette, shortcuts, toolbar, bubble, menus
  app/appCommands.ts  every command (id, title, category, keys, scope, when, run)
  app/editorMenu.ts   adaptive editor context menu (link, image, code, table, text)
  editor/format.ts    PURE formatting ops: (state) => TransactionSpec (inline toggles, headings, lists…)
  editor/table.ts     PURE GFM table model: parse, format with aligned columns, row/col edits
  editor/tableCommands.ts  table under the cursor + edit, keeping the cursor in the target cell
  editor/slash.ts     slash commands on @codemirror/autocomplete
  core/fuzzy.ts, core/keys.ts   fuzzy ranking; shortcut parsing, matching and formatting
  ui/CommandPalette.tsx, ui/Toolbar.tsx (toolbar, selection bubble, customize dialog)
  app/appMenus.ts, app/menuModel.ts   application menus as data; ui/MenuBar.tsx renders them
  app/colorThemes.ts  colour theme list (colours in styles/tokens.css)
  render/extras.ts    front matter and GitHub-style callouts for markdown-it
  editor/syntaxExtras.ts  front matter, ==highlight==, sub/sup for the editor grammar
  sync/scrollMap.ts   pure anchor building and interpolation (the scroll-sync maths, heavily tested)
  sync/scrollSync.ts  DOM driver: leader/follower, echo suppression, anchor cache, cursor marker
  core/               pure logic with no DOM and no Solid: autosave, stats, text (EOL/BOM, diff), paths
  editor/             CodeMirror extensions and the theme (colours come from CSS variables)
  platform/           host abstraction: tauri.ts (real), memory.ts (browser dev + tests)
  ui/                 Solid components (StatusBar, TabStrip, ConfirmDialog, ConflictBanner)
  styles/             tokens.css (colour themes: Snow default light, Ink default dark, and eight more), app.css
src-tauri/
  src/fsops.rs        read/stat/atomic write (temp + fsync + rename, Windows retry)
  src/kv.rs           durable key/value files in <appData>/state (drafts, session, settings)
  src/core/           feature-flagged Rust services: Mordant Markdown parsing/rendering,
                      CalmMD-derived syntax themes for Mordant, and zspell spellchecking with
                      bundled English/German/French/Spanish dictionaries plus on-demand downloads
  src/tree.rs         list_dir (folders first, natural order, skips .git/node_modules), create,
                      rename (never overwrites), trash (OS trash via the `trash` crate)
  src/windows.rs      SpaceClaims (window label → space id, released on window destroy), open_window
                      (passes the space via an initialization script), focus_window, space_window
  src/lib.rs          command registration, launch paths, single-instance handoff (emits
                      "calmmd://open-paths" to the main window)
  capabilities/       Explicit Tauri permissions: core event/menu/window, dialog, opener, clipboard
```

### Key rules
- **The UI never calls Tauri directly.** It goes through `Platform` (`src/platform/types.ts`).
  This keeps the app runnable in a browser and unit-testable with `createMemoryPlatform`.
- **One `EditorView`, many `EditorState`s.** Each tab keeps its own state (with its undo
  history) in `documents.ts`. Switching tabs calls `view.setState`. Programmatic content
  loads carry the `ExternalLoad` annotation so they don't count as edits.
- **Saving is always atomic and never clobbers.** Before writing, `persist()` stats the file. If the
  mtime moved, the document goes into `conflict`, the edits go to a draft (`draft/<id>` in kv), and the user
  chooses. Untitled documents autosave to drafts and are restored at startup.
- **Autosave** (`core/autosave.ts`) is revision-based: 1 s idle, 10 s max wait, serialised, with
  retry and backoff. `flush()` resolves to whether everything is persisted.
- Line endings and BOM are detected on open and restored on save (`core/text.ts`).
- Colours, fonts and spacing are **CSS variables only** (`styles/tokens.css`). The editor theme
  reads them, so themes switch without rebuilding extensions.

### Conventions
- TypeScript strict, no `any` in new code. Pure logic goes in `core/` with a `*.test.ts` next to it.
- Keep dependencies few and permissively licensed (MIT/BSD/Apache/ISC; OFL for fonts).
  Record new runtime dependencies in `THIRD_PARTY_LICENSES.md`.
- Clean-room: never copy code, assets or CSS from Typora, Markdown Monster, Zettlr (GPL) or Obsidian.
- UI copy is short and calm ("Saved", "Changed on disk"). No product branding inside the editor.
- Shortcuts use `Mod` = Cmd on macOS, Ctrl elsewhere.
- Solid stores *merge* objects on `setState(key, obj)`. To replace a record, use `reconcile(obj)`.
- Don't read a `<Show>` child accessor after the condition may have turned false (Solid throws
  "stale read"). Capture values first.
- Don't rewrite source files with Windows PowerShell 5.1 `Get-Content`/`WriteAllText`. It corrupts UTF-8
  (`…` → `â€¦`). Use the Edit tool or a Node script.

### Live mode (don't regress its feel)
- Reveal rule: syntax shows while any selection range *touches* the element (edges inclusive). Line
  syntax (headings, quotes, rules) reveals while the selection is on that line.
- **No layout jumps on reveal.** Styling that changes height (heading size and padding, code-block
  padding) lives on *line* decorations that don't depend on the cursor. Only the markup characters
  toggle. Images keep a companion picture after the revealed source. Tables and HTML blocks
  (rendered ↔ source) are the accepted height changes.
- Live mode is a `Compartment` per EditorState (App `applyLiveMode`): new states start in the current
  mode, and others are reconfigured on activation.
- Heading token sizes use `--cm-h1..3` variables, set to `1em` in live mode, so line font sizes don't compound.
- Widgets that handle clicks must return `false` from `ignoreEvent`, or CM never calls the
  domEventHandlers.
- Inside block widgets, reset `white-space: normal` (CM content is pre-wrap).
- Theme variables (`--md-*`) are on `:root` with `[data-preview-theme]` overrides. Never redefine them
  on `.md-body`, or nested widgets reset to Paper.
- **Files must read the same on GitHub.** Never render single newlines as breaks. Instead Enter in a top-level
  paragraph writes a blank line and Shift+Enter a trailing `\` hard break (`editor/paragraphs.ts`, Live mode only;
  lists, quotes and code keep the usual Enter). The analyzer hides the backslash of a `HardBreak`.
  Live mode draws a blank line between top-level blocks at the preview's paragraph gap (`cm-live-gap`, full height
  under the cursor), so Enter doesn't look like an empty line.
- **Formatting over formatting is restyled, not wrapped.** Bold, italic, strike, highlight and colour on a selection
  go through `editor/restyle.ts`: the affected part of the line is read as runs of styled text (from the syntax tree
  and the colour tags), the change is applied to the selected characters only, and the part is written back with
  nested marks, so tags never cross. Code, links, math and unknown spans stay whole. It needs the syntax tree, so
  plain states (unit tests, .txt) fall back to simple wrapping.
- **Enter steps out of inline formatting.** When everything after the cursor is closing marks (`**`, `_`, `==`,
  `</span>`, …), `editor/inlineEnter.ts` (highest precedence, every mode) moves past them, or removes an empty pair,
  then returns false so the usual Enter runs from there.
- **Line prefixes move the cursor.** Headings, lists and quotes (`editor/format.ts`) map the selection with assoc 1,
  so a cursor at the start of the line ends up after the new marker.
- **.txt files are plain text.** `isPlainTextPath`: the tab shows in Source mode with no Markdown language
  (a per-state compartment), no toolbar, and the view switch disabled. The window's chosen mode is kept for other tabs.
- **Text width** (View → Text Width) scales each document style's measure with `--measure-scale` (editor and preview,
  not exports).
- **Saved files appear in the sidebar.** `platform/savedFiles.ts` wraps the platform: a path picked in a save dialog
  refreshes its folder once written (Save As, exports, share bundles).
- Measured: 2,201 lines with 200 tables at a 4 ms median and 7 ms max per keystroke, including
  decoration and DOM update.

### Commands
- **Add features as commands** in `appCommands.ts`. Then they're in the palette, can be bound to a
  key, and can be put in the toolbar (add an icon to `ui/formatIcons.tsx`) or a context menu.
- `scope: 'editor'` shortcuts are matched inside CodeMirror (Prec.high domEventHandlers, which look up
  the registry at key time). `global` ones are matched on window keydown, which skips events
  CodeMirror already handled (`defaultPrevented`).
- Shortcut matching uses physical keys (`e.code`) for letters and digits, so it works with Alt/Option and
  non-US layouts.
- Formatting commands dispatch with `userEvent: 'input.format'`. The selection bubble stays open for those.

### Windows and Spaces
- Where a window's session lives: its Space (`spaces.saveSession`), or `session` for a loose
  **main** window. Extra loose windows are scratch and don't persist a layout. `main-space` records
  which Space the main window reopens.
- Draft ownership: `draftOwner()` is `space:<id>` or `window:<label>`, and restore only takes owned drafts.
  The loose main window also adopts orphans (no owner, or windows that aren't open). After the owner
  changes (Save as Space), call `docs.rewriteDrafts()`, or drafts leak to the old owner.
- Switching: `leaveCurrent()` (flush docs and session, `sessionReady=false`, `docs.reset()` which keeps
  drafts) → claim → `loadSession()` → `sessionReady=true`.
- View mode is a per-window signal (`mode()`). `settings.viewMode` is only the default for new windows.
- Cross-window: `platform.broadcast/onBroadcast` ("settings", "favorites", "spaces"). Browser dev
  simulates windows as tabs (`?window=label&space=id`) over a BroadcastChannel, but claims aren't shared
  between tabs there.

### Export (PDF quality is a priority, so read before changing)
- `export/build.ts`: `buildBody` (preload grammars and diagrams, render, strip editor attributes,
  embed images, embeds → players or links, TOC) and `buildDocument` (one self-contained page, with KaTeX
  CSS and fonts only when the body has math). `export/assets.ts` lazy-loads fonts (woff2 `?inline`), CSS
  (`?raw`), KaTeX fonts (`import.meta.glob`) and Paged.js. Paged.js is a *relative* file import, because
  an alias gets caught by Vite's dev dependency optimiser. `export/takumi.ts` maps the same HTML and
  PDF preferences to Takumi. Desktop exports use `core/export/takumi.rs` (native Rust); the browser
  build and tests keep the local WASM fallback (neither engine uses a browser print process, TeX or
  the network). Takumi is the default PDF engine; `calmmd.pdf.engine = 'browser'` selects the older
  Paged.js/Chromium path.
- **Paged.js drops custom-property declarations** when it re-parses CSS. Theme CSS therefore goes in
  `<style data-pagedjs-ignore>`, and only `@page`, break and target-counter rules go through Paged.js.
- The page sets `window.__calmmdDone` when ready. `src-tauri/src/cdp.rs` waits for it over the DevTools
  protocol, then calls `Page.printToPDF`. Don't go back to `--virtual-time-budget`: it's racy.
- Grammars are lazy, so `buildBody` calls `preloadLanguages` before rendering.
- Under Vitest, CSS `?raw` imports are empty, and `new URL(\`…${x}\`, import.meta.url)` is rewritten.
  `pdf.smoke.test.ts` reads CSS from disk.
- Manual end-to-end check:
  `CALMMD_EXPORT_OUT=<dir> npx vitest run src/export/pdf.smoke.test.ts`, then
  `CALMMD_PRINT_HTML=<dir>/print-paper.html CALMMD_PRINT_OUT=<dir>/out.pdf cargo test --manifest-path src-tauri/Cargo.toml prints_file_from_env -- --include-ignored`.
  `cargo test -- --include-ignored` also runs the real pagination test (needs Edge or Chrome).
- User themes: `app/themes.ts`. The CSS is injected into `<style id="calmmd-user-theme">`, and
  `data-preview-theme` always holds the *base* style (`activeBase()`).

### Math, diagrams, embeds, spellcheck
- **Trusted HTML:** KaTeX and Mermaid output bypasses DOMPurify (which strips MathML and SVG `<style>`)
  through numbered slots: `trusted(env, html)` returns `<span data-trusted="<nonce>-i">`, and
  `createRenderer` swaps slots back in after sanitising. The nonce is per render, so document HTML
  can't forge a slot. Never put user-controlled HTML in a slot.
- `core/mathScan.ts` is the one `$` scanner, shared by markdown-it (`render/math.ts`) and the Lezer
  extension (`editor/mathSyntax.ts`, InlineMath and BlockMath nodes).
- `render/diagrams.ts`: lazy Mermaid, serialised renders, cache keyed by (theme, source), `htmlLabels: false`
  (pure SVG, so it survives export). **elkjs (EPL/GPL) is aliased to `render/elk-stub.ts`** in vite.config,
  and `layout: 'dagre'` must stay explicit: this Mermaid version otherwise picks ELK for flowcharts. The renderer shows the source while pending and re-renders via onAsyncUpdate.
- `render/embeds.ts`: `parseEmbed`, click-to-load cards, `activateEmbed`. `patch.ts` `rememberOriginal()` keeps a
  playing embed from being replaced on re-render.
- Live mode: InlineMath → MathWidget. BlockMath and mermaid fences → block widgets, with
  `math-preview`/`diagram-preview` companions while editing (analyzeBlocks). Video images → EmbedWidget,
  which handles its own click because `ignoreEvent` hides it from CM. `refreshLive` effect re-themes diagrams.
- Spellcheck is a compartment (`spellCompartment`) with contentDOM `spellcheck` and `lang` attributes.
  Where the OS spellchecker answers (`src-tauri/src/spell.rs`: Windows ISpellChecker, macOS NSSpellChecker),
  CalmMD draws the underlines itself (`editor/spelling.ts`: visible lines checked in one `spell_check_text`
  batch, cached by line text, code/links/math skipped, the word being typed left alone) and turns the
  webview's checker off, because its underlines vanish whenever Live mode redraws a line. The right-click
  menu lists `spell_check` suggestions and Add to Dictionary (`wordAt`, `spellingItems` in
  `app/editorMenu.ts`). Tools → Spelling Language combines locally available dictionaries (●) and
  downloadable ones (↓) in one menu; installing one refreshes the list and moves it into the first
  group. On Linux there's no OS checker to ask: the webview underlines, and its native menu
  (Shift+right-click) has the suggestions.
- Debugging real exports: run the `web-debug` launch config (sets `CALMMD_DEBUG_DIR`), build the HTML in the page
  (`buildBody`/`buildDocument` via dynamic import), `POST /__calmmd/save?name=x.html`, then print it with
  `prints_file_from_env`.

### Drawings (Excalidraw)

- A drawing is its own file next to the notes, `Name.excalidraw.svg`: an SVG picture with the editable scene
  embedded (`exportEmbedScene`), so GitHub and other viewers show it and CalmMD can open it again. Plain
  `.excalidraw` JSON files open too and save as JSON. Helpers: `core/drawings.ts`; `fileKind` returns `'drawing'`.
- Insert → Drawing (also `/drawing`) and File → New Drawing ask for a name, then open `ui/DrawingEditor.tsx`
  full window. It autosaves like a note; a new drawing's file is only written once something is drawn, and
  Insert puts the image link in the note on that first save. Double-click a drawing's picture in Live mode,
  "Edit Drawing" in its right-click menu, or click it in the sidebar to edit it.
- The editor is React (`drawing/excalidraw.ts`, no JSX because the app's JSX is Solid), lazy-loaded, so neither
  React nor Excalidraw is in the startup bundle.
- **Offline fonts:** `window.EXCALIDRAW_ASSET_PATH = '/excalidraw/'`, and the `excalidrawFonts` Vite plugin serves
  and bundles `dist/prod/fonts` there, except Xiaolai (CJK, 13 MB). Excalidraw always appends an esm.sh CDN
  fallback; the plugin rewrites it to the app's own origin in builds, so nothing is fetched online.
- CSP: `'wasm-unsafe-eval'` (font tools) and `worker-src 'self' blob:`. Excalidraw's glyph subsetting needs
  `unsafe-eval`, which we don't allow, so it falls back to embedding whole fonts (about 30 KB each).
- After a save, `withAssetVersions` (`platform/assetVersions.ts`) adds `?v=n` to that file's URL and
  `imagesChanged()` makes Live image widgets redraw, so notes show the new picture.
- Browser checks: on `?demo` pages the Excalidraw API is on `window.__excalidraw` (and
  `__excalidrawConvert`), because synthetic clicks don't reach Excalidraw's canvas. `?doc=lisbon` opens a
  note with a picture.
- **Tabs:** drawings, images, PDFs and other files open as file tabs after the note tabs (`FileTab`, `TabStrip`),
  shown in the main area instead of the editor (`DrawingEditor embedded`, `QuickView embedded`). Note tabs stay in
  `documents.ts`; file tabs are a list in App with `activeFile`. Switching or opening a note clears `activeFile`;
  Mod+W closes the file tab shown. A drawing saves pending changes when its tab closes or loses focus. On the first
  save of a new drawing, the note that asked for it gets the link (`waitingForDrawing`). Space in the sidebar still
  opens a Quick view peek.

### Local MCP server and Rust workspace

See [`MCP.md`](MCP.md) for the full implementation record, tool reference and open topics, and
[`MCP-CLIENT-SETUP.md`](MCP-CLIENT-SETUP.md) for client configuration and discovery.

CalmMD can host a **local MCP server** (`crates/calmmd-core/src/mcp/`, official `rmcp` SDK, Streamable HTTP on
`127.0.0.1:<random port>/mcp`). It is **off until started** from File → Start MCP Server, the command
palette, or the `--mcp` launch flag. The connection URL and a per-launch bearer token are copied to
the clipboard; the server validates the token and the `Origin` header and never binds beyond loopback.

Most tools call `crates/calmmd-core/src/core/workspace.rs` directly. The webview mirrors live buffers into
that Rust state with `core_workspace_sync` / `core_workspace_upsert_document`, and Rust emits
`calmmd://workspace` events for content changes, renames, creates and trashes. The remaining
`docs`/`explorer` controllers apply those events so CodeMirror, favorites and the tree stay in step.
Autosave now runs in `crates/calmmd-core/src/core/autosave.rs`: the webview reports edits with
`bufferAutosaveTouch`, and Rust owns debounce/max-wait, retries, conflict resolution
(`keepMine` / `loadTheirs`) and `calmmd://autosave` status events. Writes and conflict-safe drafts
are in `crates/calmmd-core/src/core/buffers.rs`. Session and Space
records are validated and stored by `crates/calmmd-core/src/core/session.rs`; the webview still builds the
snapshot from CodeMirror cursors and the sidebar, but reads and writes it through `Platform`.
The webview keeps its immediate CodeMirror undo stack and the explorer view. File history is
stored and queried through `core/history.rs`. PDF/HTML export resolves paths, live text, options
and cover defaults in Rust; the remaining webview bridge only renders DOM/Mermaid and assembles
the final document. It still emits a
request (`calmmd://mcp/request`) with a correlation id; `src/app/mcpBridge.ts` dispatches it to the
exporter and answers with the `mcp_respond` command. Paths are resolved against the open Space root and
`..`/outside-root paths are rejected in the bridge. Every tool accepts an optional `windowLabel`
or `spaceId`; the latter is resolved through `SpaceClaims`, so calls can be routed to the window
that holds a Space. Without a target they go to `main`. The server can be started from File → Start
MCP Server, the palette, `--mcp`, or the persisted **Auto-start MCP Server** setting; a small `MCP`
marker appears in the status bar while it is running.

Tools: `calmmd_status`, `calmmd_windows`, `calmmd_list_documents`, `calmmd_read_document`,
`calmmd_search`, `calmmd_outline`, `calmmd_create_document`, `calmmd_create_folder`,
`calmmd_rename_document`, `calmmd_move_document`, `calmmd_write_document`,
`calmmd_undo_document` / `calmmd_redo_document`, `calmmd_trash_document`,

`calmmd_triage_folder`, `calmmd_apply_organization`, `calmmd_export_pdf`, `calmmd_export_html`.
Prompts: `calmmd_summarize`, `calmmd_follow_up_guide` (live note → client-side LLM instruction,
written back with `calmmd_write_document`) and `calmmd_organize_folder` (live folder inventory →
JSON move plan applied with `calmmd_apply_organization`).
Read-only resources: `calmmd://status`, `calmmd://windows`, `calmmd://document/{path}` and
`calmmd://folder/{path}` (percent-encoded paths; optional `windowLabel`/`spaceId` query).
Reads use the in-memory buffer for open tabs (so unsaved edits are visible). Writes go through
`docs.setContentByPath()` with `userEvent: 'input.mcp'`, snapshot the old and new text into file
history, and flush the atomic save. Triage inventories live note text; organisation plans are
validated, then `explorer.move()` / `explorer.rename()` create folders, move notes and retarget
tabs, favorites, history and links with `rewriteLinks()`. The AI client generates summaries
and follow-up guides and writes them back with `calmmd_write_document`; CalmMD has no built-in LLM.

The desktop build's MSRV is Rust 1.91 (edition stays 2021): `rmcp` 3.11 requires 1.88 and the
Takumi PDF crates require 1.91. CI and release builds use stable Rust, which is newer than 1.91.

### New-version notices

- Asked once (`settings.updateCheck`: `'ask'` → `'on'`/`'off'`, a confirm 4 s after the first window opens;
  Esc asks again next launch). When on, the main window fetches GitHub's `releases/latest` about once a day
  (`app/updates.ts`; CSP allows `connect-src https://api.github.com`) and shows a sticky status bar notice with
  "What's New" (the release page) once per version. Help → Check for Updates… checks on demand; Help → Tell
  Me About New Versions toggles it. Nothing else is sent. The release notes on GitHub are where new features
  are announced.

### File history size

- Unnamed versions are capped per file (`HISTORY_KEEP_UNNAMED`); saving on close is off by default. The History
  tab shows total usage (`history.usage()`) and can delete one file's versions or all of them (also File →
  Clear All File History… in the command palette).

### Files and sessions
- Favorites are global (all windows). The root, expanded folders, tabs and sidebar are per-window session state,
  and Spaces bundle them.
- Explorer rows under Favorites that also appear in the main tree are *mirrors* and never host inline
  editors, so only one input exists.
- Renames go explorer → `docs.renamePaths` (retargets open tabs). Trash goes `docs.closeWithin`, which closes
  clean tabs and keeps edited ones (they're recreated on the next save).
- Session restore reopens tabs by path, reports missing files once, and restores scroll *by line* across restarts
  (pixel offsets are only reused within one run).

### Scroll sync (key differentiator, so don't regress it)
- Every rendered block has `data-line-start`/`data-line-end` (0-based, end exclusive). The Rust
  source-map pass reproduces the markdown-it maps, checked against a 500-line mixed fixture.
- Anchors pair editor Y (CodeMirror `lineBlockAt`) with preview Y (block rects): block top ↔ first-line
  top, and block bottom ↔ last-line bottom, *including one-line blocks* (tall images). The ends of the two
  scroll ranges are aligned. Flat segments are allowed (a table's `|---|` line has no rendered height).
- Anchors are cached and rebuilt when renders, resizes, image loads or CM height changes happen, *or* when either
  scroll extent changes (a catch-all for layout changes we don't hear about).
- The leader is the pane with the last user input (wheel, pointer, key, touch). The follower's resulting
  scroll event is ignored as an echo. `.pane-preview` has `overflow-anchor: none`.
- Images are loaded eagerly on purpose. Lazy images pop in and shift the layout.
- Measured in the browser dev build with a 500-line mixed document (tall images, tables, code):
  the top-of-viewport source line matches to within 0.03 lines in both directions. The fixture and
  its frozen anchor baseline are in `scroll-sync-500.test.ts`; the Rust renderer must emit the same
  anchors.

### Git workflow
`main` is always releasable. Work happens on short-lived feature branches merged by pull request.
Commit messages use the imperative mood with a short subject.

## Testing notes
- `documents.test.ts` covers conflicts, CRLF/BOM round-trips, draft restore, and closing behaviour
  against the memory platform. Add a case there for any new save or close path.
- Autosave tests use fake timers (`vi.advanceTimersByTimeAsync`).
- In the browser dev build, `window.__calmmd` exposes `{ view, preview, docs, settings }`.
- When driving the dev build from an automated, *hidden* browser pane, rAF and scroll events barely
  fire. Shim `requestAnimationFrame` with `setTimeout`, reset `__calmmd.preview.sync.frame = 0` and
  `view.measureScheduled = -1`, and dispatch `scroll` events by hand.


## Menus, themes, images, links, extras
- Menus are data (`appMenus.ts`): mostly command ids, resolved by `resolveMenu` (title, shortcut, `when`). Add a
  feature as a command first, then list it in a menu. The in-window bar is hidden where `platform.nativeMenu` is
  true (macOS desktop). The native menu has **no accelerators** on purpose: the webview already handles every
  shortcut, and a native key equivalent could run toggles twice. Cut/Copy/Paste/Select All are the OS's
  predefined items there, or the keys don't reach the webview. The native menu is untested on a real Mac.
- Colour themes: `<html data-theme="light|dark" data-color-theme="<id>">`, set by App from `colorScheme`,
  `lightTheme` and `darkTheme`. `:root` is Snow (and what exports use) and `[data-theme='dark']` is Ink. Every other
  theme overrides the full colour set in `tokens.css`. Add a theme in `tokens.css` and `app/colorThemes.ts`.
- Document styles (`previewTheme`, ids are stable): paper ("Book"), studio, clear, github, typewriter.
- `settings.v` (SETTINGS_VERSION = 2): older settings move to the new defaults once (split view, full toolbar), and
  App also opens the sidebar and sets split view in the loaded session. Bump the version and extend
  `parseSettings` for future default changes.
- Live mode now covers front matter (`FrontMatter` node, `core/frontMatter.ts` shared with the outline and the
  renderer), `==highlight==` (custom Lezer inline), Lezer's Subscript/Superscript, escapes, reference links
  (definitions via `linkRefs`), footnote refs, paired inline HTML (`<kbd>`, `<mark>`, …), HTML blocks as block
  widgets (grouped up to the closing tag, so `<details>` with Markdown inside works), callouts, nested-quote depth,
  and code fences that fade to a language label. The renderer adds markdown-it-mark/sub/sup, front matter
  (hidden in exports) and callouts (`render/extras.ts`).
- Highlight and text colours (`editor/colors.ts`, pure, tested): written as inline HTML (`<mark style="background: …">`,
  `<span style="color: …">`) so they survive elsewhere; the default highlight stays `==text==`. The palette (6 + 6) is
  chosen to read on light and dark themes. Live mode copies only safe colour declarations (`colorStyleOf`).
  Toolbar split buttons reuse the last colour (`settings.highlightColor`/`textColor`). Settings v3 adds the text
  colour button to v2 toolbars; only upgrades from before v2 reset the layout (`LAYOUT_VERSION`).
  Picking a colour with nothing selected never inserts anything: highlights just remember the colour, and text
  colour arms `editor/typingColor.ts` (the next typed text is wrapped, the cursor shows the colour).
- Images (`core/assets.ts` pure + tested, `app/assets.ts` does the file work): pasted, dropped (Tauri file drop or
  HTML5) and picked images are saved as files and linked **relative to the document**, in `<doc folder>/assets/` or
  `<root>/assets/` (`settings.imageFolder`; root falls back outside the root). Screenshots are named by content hash
  (`image-<12 hex>.png`, so repeats reuse one file); named files keep a cleaned name with `-2` on clashes. Untitled
  documents link absolute paths in `<appData>/pending-assets`; `documents.saveAs` calls `beforeSaveAs` →
  `assets.relocate`, which copies pending and folder-owned images next to the new file and rewrites links (others
  are re-pointed). Paste ignores images when the clipboard also has plain text (Office puts a picture of the text
  there). Binary writes use Tauri's raw IPC body with the path in a header (`write_binary_file`).
- Search in folder: `core/search.ts` (match rules, browser/test implementation) mirrored by `src-tauri/src/search.rs`
  (`search_files`: Markdown files only, skips hidden/tool folders, natural order, 500 hits max). The panel lives in
  the sidebar (`ui/SearchPanel.tsx`, kept mounted so the query survives tab switches).
- Links between notes (`core/noteLinks.ts`, pure + tested): the `[[` picker (`editor/notePicker.ts`, an extra source
  in the slash autocomplete) inserts **relative Markdown links**. `[[Wiki|alias]]` / `[[Note#Heading]]` render as
  `wiki:` links (markdown-it `wikiLinkPlugin`, Lezer `WikiLink`, DOMPurify allows `wiki:`); `openLink` hands them to
  App `openWiki`, which resolves via `resolveWiki` (same folder, then closest in the open folder). Exports turn them
  into plain text. "Linked from" (Outline tab) prefilters with `searchFiles`, then checks links with `linksTo`.
- CalmMD is entirely free: there is no paid tier, licence key or store, and no feature is gated. File history,
  custom colours, the PDF cover page and templates, every colour theme and share bundles are always available.
  PDF cover page: `core/cover.ts` + `ExportOptions.cover`; in PDFs it's a named page
  (`page: cover`) without header or page number, and its title is a `<p>` so heading rules skip it.
- Image size and alignment live in the alt text (`core/imageSize.ts`): `![alt|300](…)`, `|300x200`, `|center`,
  `|right`, in either order. The renderer turns them into width/height and `img-center`/`img-right`; Live mode puts
  `align-*` on the widget. Drag handles (Live: `widgets.ts` `resizeHandle`; preview: `app/previewImageResize.ts`,
  which finds the source by `data-line-start` + image index) and the image menu all write through `withAltOptions`.
- All files show in the sidebar (`settings.showAllFiles`); non-notes open in Quick view (`ui/QuickView.tsx`,
  `core/fileKinds.ts`): images, PDFs (iframe, `frame-src asset:` in the CSP), text/code, CSV/TSV, details.
- File history (`app/history.ts` + tests, `core/diff.ts` Myers line diff): versions live in the kv store,
  never in the user's folders. `history/map` maps `pathKey(path)` → a random file id, so renames (`explorer.onRenamed`)
  only touch the map; `history/<id>/index` lists versions and `history/<id>/<version>` holds the text. Unnamed
  versions are deduped against the latest and pruned to 50; named ones are kept. Capture: `documents` remembers each
  file's text as opened and calls `onClosing(path, opened, current)` when a changed tab closes, and
  `takeClosingVersions()` on window close and Space switches; App saves "opened" (if new) and "current" when
  `settings.historyOnClose` is on. Restore first saves the current text, then applies a `minimalChange` (undoable).
- Zoom: `settings.fontScale` (status bar control, Mod+=/-/0, Ctrl/Cmd+wheel). Font: `settings.fontFamily` sets
  `data-font` on <html>, which overrides the document style font (preview.css). Exports still use the style font.
- Browser demo: `?demo&md=<base64url>` opens any document; `&ui=appearance|menu:View/Color Theme|spaces|folder`
  opens that UI; `&light=…&dark=…&toolbar=0`. Headless captures need `--force-prefers-reduced-motion` (menus
  fade in) and can't show the sidebar's context menus (they close when the headless window blurs).

## Release and docs
- Versions live in three places: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`. Keep them in sync.
- `?demo` (browser build only, `platform/demo.ts`) loads a showcase folder and layout. Params: `mode`, `scheme`,
  `theme`, `sidebar` (files|outline|none), `palette` (opens the palette with a query). README screenshots come from
  `node scripts/screenshots.mjs` with `npm run dev` running. They're 2880×1800 (2× DPR) in `docs/screenshots/`.
- `npm run preview` (port 4173, launch config `web-prod`) serves `dist` with the app's real CSP from
  `tauri.conf.json`. Use it to check that new features don't need CSP changes. Production builds have no `__calmmd`
  global, so reach the editor via `document.querySelector('.cm-content').cmTile.view`.
