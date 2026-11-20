# CalmMD — Product Spec

> *Markdown, calmly.*

CalmMD is a calm, beautiful, open-source desktop Markdown editor for macOS,
Windows, and Linux. It has power-user features but stays visually quiet. Rendering
quality should feel like a premium writing app, file handling should be practical,
and PDF output should be good enough to hand in.

License: **MIT**. Status: pre-1.0, built in milestones (see bottom).

---

## 1. Brand

| Item | Decision |
|---|---|
| Name | **CalmMD** (pronounced "calm M-D"). Written as `CalmMD` in prose and `calmmd` in ids. Short real-word names (Plume, Margin, Tinta, Inkling, Featherdown, Calmark…) are all taken by existing Markdown editors. Known near-collision: the hobby repo `suheilOS/calmd`. The name lives in `apps/calmmd-gpui/app.json`; the legacy webview shell still has `tauri.conf.json`. |
| Tagline | "Markdown, calmly." |
| Mark | A lowercase **m** whose last stroke softens into a downward curve (a nod to Markdown's ↓), in a rounded square. It works as a monochrome glyph at 16 px. |
| Palette | Ink `#1F2A44` · Paper `#FAF8F4` · Amber accent `#E8A23B` |
| Voice | Plain, warm, brief. "Saved." not "Your document has been saved successfully!" |
| Short id | `calmmd` (bundle id `app.calmmd.editor`) |
| Branding in-app | **None inside the editor.** The name appears only in the title bar, About dialog, and installer. No splash screen. |

## 2. Design principles (these override feature requests)

1. **Calm UI.** Minimal chrome by default. Panels never open on their own. Every panel can be toggled and remembers its state.
2. **Progressive disclosure.** A small curated toolbar, with everything else in the
   command palette (`Ctrl/Cmd+Shift+P`), an adaptive context menu, a selection
   bubble, and `/` slash commands. Users choose which buttons appear in the toolbar.
3. **Subtle branding.** See above.
4. **Non-goals for v1:** git integration, AI chat, screen capture, plugin marketplace, sync/cloud, accounts, telemetry.
5. **Typography first.** Intentional light and dark themes with good default fonts and comfortable measure (~68ch) and rhythm.

## 3. Vocabulary

- **Document**: one `.md` file open in a tab.
- **Space**: what other apps call a "workspace". It is a saved bundle of a root folder, its
  open tabs, layout, view mode, and visible panels. One window shows one Space.
  "Space" is short and reads naturally in commands ("Switch Space…", "Open Space in New Window").
- **View mode**: Source · Live · Split · Preview.

## 4. Features (v1)

### 4.1 Editing and viewing
- **Four view modes**, cycled with `Ctrl/Cmd+E` or chosen with `Ctrl/Cmd+Alt+1..4` (`Ctrl/Cmd+1..9` is kept for tabs), plus one
  segmented control in the status bar.
  - *Source*: CodeMirror with Markdown highlighting.
  - *Live* (**the default**): the hybrid mode. Markdown renders inline (headings, emphasis, links, images,
    task boxes, tables, math, code fences). The raw syntax for an element reappears when
    the cursor or selection touches it. Revealing never changes line heights, except tables
    switching between rendered and source. Links open with `Mod+click`, and checkboxes toggle on click.
    Clicking a table row puts the cursor in that row.
  - *Split*: source on the left, preview on the right, with a draggable divider.
  - *Preview*: the rendered document, read-only.
- **Split-view sync** (key differentiator), bidirectional and toggleable:
  - Every rendered block carries `data-line-start` / `data-line-end`, taken from markdown-it token maps.
  - A *sync map* is a sorted list of anchors `(sourceY, previewY)`, built from the editor's line
    block positions and the preview element offsets. The two pane positions map through
    piecewise-linear interpolation between neighbouring anchors. It is never a percentage of scroll.
  - Anchors are rebuilt after renders, resizes, and image/Mermaid/KaTeX load events.
  - The pane the user is scrolling leads; the other follows on the next animation frame. Feedback loops are suppressed.
  - Cursor sync: moving the caret in source highlights and reveals the matching
    preview block, and clicking a preview block places the caret at its source line.
- **Spellcheck** on by default, using the local zspell service where a dictionary is available and
  the OS/webview checker otherwise ("Toggle Spellcheck" and "Spelling Language: …" in the palette).
  English (US/UK), German, French and Spanish are bundled. Italian, Portuguese, Dutch/Flemish,
  Finnish, Norwegian, Danish, Polish, Chinese, Japanese, Korean and Russian are downloaded on
  explicit request from Tools → Spelling Language (↓ marker) and then join the available group (●).
  The language sets the editor's `lang` attribute. "Automatic" uses the system languages. Spelling
  suggestions are in the native menu: **Shift+right-click** (our menu says so).
- **Math**: `$…$` inline and `$$…$$` blocks with KaTeX, using Pandoc rules, so "$5 and $10" stays text.
  The editor grammar and renderer share one scanner. In Live mode math renders in place, and while
  editing a block a live preview sits under the source.
- **Mermaid**: ```` ```mermaid ```` fences render as diagrams in the preview, Live mode (with a live
  preview while editing), and exports. Mermaid is lazy-loaded and results are cached per source and theme.
- **Status bar**: words · characters · lines, or the selection counts when text is selected.
  It also shows save state and view mode.
- **Embeds**: pasting a YouTube, Vimeo or video-file URL on its own line offers "Embed" in the
  status bar (non-modal). The embed is stored as plain Markdown image syntax with a
  recognised URL (`![video](https://youtu.be/…)`), so files stay portable. Preview and Live mode
  render a quiet card that loads the player only on click (`youtube-nocookie.com`, Vimeo `dnt=1`). No
  third-party request happens before that. A playing video survives re-renders. Local and remote
  video files get a native `<video>` player. HTML export embeds players, and PDF export shows links.
- **Autosave**: debounced (1 s idle, 10 s max), crash-safe atomic writes (write to a temp
  file in the same directory, fsync, rename). An explicit `Ctrl/Cmd+S` saves immediately. Untitled
  documents keep a recovery copy in app data until they are first saved. If the file on disk
  changes underneath an unmodified buffer, the buffer reloads silently. If there are unsaved
  edits, a quiet banner offers Keep mine / Load theirs.
- **Markdown**: CommonMark + GFM (tables, task lists, strikethrough, autolinks),
  footnotes, KaTeX math (`$…$`, `$$…$$`), syntax-highlighted code fences, Mermaid diagrams, and front matter (shown dimmed, not rendered).

### 4.2 Files and organization
- **File explorer** sidebar for the Space root: lazy tree, rename, new file/folder, reveal in OS, delete to trash.
- **Favorites** for files *and* folders, pinned at the top of the sidebar. They're global (shared by all
  Spaces), and favorite folders expand inline as mini-trees.
- The explorer shows Markdown files and folders by default ("Show All Files" is in the root's context menu).
  It skips dotfiles, `.git` and `node_modules`. Deleting always goes to the OS trash.
- Sidebar shortcuts: `Mod+\` toggles it, `Mod+Shift+E` shows Files, `Mod+Shift+O` shows Outline.
- **Outline** panel built from headings. Clicking jumps to the heading, and the current heading is highlighted.
- **Tabs** persist across restarts (session restore, including scroll and cursor).
- **Spaces**: a named, saved window setup (folder, tabs with cursors, expanded folders, sidebar layout,
  view mode).
  - "Save Window as Space…" creates one. Switch with `Mod+Alt+O` or `#` in the palette (`Mod+Enter`
    opens it in a new window). Recent Spaces are also listed in the empty sidebar.
  - A Space is open in at most one window. Choosing one that's already open focuses that window.
  - The main window reopens its last Space at startup. Extra loose windows (`Mod+Shift+N`) are
    scratch windows, and their layout isn't kept (unsaved drafts are adopted by the main window).
  - Untitled drafts belong to their Space and come back only there. Deleting a Space never touches files.
  - Settings and favorites are shared by every window and update live.
- **Single instance**: opening a file while CalmMD runs hands it to the running app.

### 4.3 Export and styling
- **HTML export**: a standalone file with the current preview theme inlined. Images can optionally be inlined as data URIs.
- **PDF export**: see §6 for the engine decision. It supports page size and margins, headers and footers
  (title, date, page X of Y), an optional table of contents, page-break control (`---pagebreak---`
  directive and `break-before` on H1 as an option), and correct math, code, Mermaid, and images.
- **Themes**: CSS files in `<appData>/themes/`, used for both the preview and exports. There are two
  independent axes:
  - *Colour scheme*: **Paper** (light) and **Ink** (dark). It follows the OS by default and can be overridden.
  - *Preview style* (built-in, each works in light and dark): **Paper** (serif, book-like, default),
    **Studio** (sans, technical), and **Typewriter** (monospaced).

### 4.4 Platform
macOS (universal), Windows (x64, arm64), and Linux (AppImage, deb). All three are built in CI from M9.

## 5. UX decisions

- **Default chrome**: tab strip, editor, status bar. The sidebar is hidden on first run and remembered after that.
- **Toolbar**: off by default. When enabled it is a single slim row of ≤ 8 user-chosen buttons.
- **Selection bubble**: appears after a short delay (150 ms) on a non-empty mouse selection.
  It offers bold, italic, code, link, strike, and heading ▾. It never appears in Preview mode or during keyboard selection.
- **Context menu**: custom-drawn and adapted to what is under the cursor:
  text → format/insert; link → open/copy/edit/unlink; image → open/copy path/alt text;
  table → insert row/column, align, delete; code block → language, copy, select contents.
- **Slash commands**: `/` at the start of an empty line opens a small inline list (heading,
  table, code, quote, task list, math, mermaid, divider, embed). Escape or continuing to type dismisses it. It can be turned off in settings.
- **Command palette**: fuzzy search over every command, recent files, and Spaces. Each entry shows its shortcut.
- **Motion**: 120–180 ms ease-out transitions. Animation stays off when the OS asks for reduced motion.
- **Fonts** (bundled, OFL): *Literata* for serif preview, *Inter* for UI and sans preview, *JetBrains Mono* for code.

### 5.1 Default shortcuts (Mod = ⌘ on macOS, Ctrl elsewhere)

| Action | Shortcut | Action | Shortcut |
|---|---|---|---|
| Command palette | Mod+Shift+P | Go to file | Mod+P |
| Go to line / heading | Mod+G / Mod+Shift+H | Find & replace | Mod+F |
| New / Open / Save / Save as | Mod+N / O / S / Shift+S | Close / reopen tab | Mod+W / Mod+Shift+T |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab | Tab 1–9 | Mod+1…9 |
| View modes | Mod+Alt+1…4, cycle Mod+E | Sidebar / Files / Outline | Mod+\ / Mod+Shift+E / Mod+Shift+O |
| Bold / Italic / Strike / Code | Mod+B / I / Shift+X / \` | Link | Mod+K |
| Heading 1–6 / Paragraph | Mod+Shift+1…6 / Mod+Shift+0 | Bullets / Numbers | Mod+Shift+8 / Mod+Shift+7 |
| Task (toggle) | Mod+Enter | Quote | Mod+Shift+. |
| Code block | Mod+Alt+C | Text size | Mod+= / Mod+- / Mod+0 |

- In the palette, a prefix switches modes: `>` commands, `:` line number, `@` heading, nothing = files.
  Recently used commands come first.
- `Shift`+right-click shows the native context menu (including spelling suggestions).

## 6. PDF engine decision

Options considered:

| Option | Quality | Install pain | Notes |
|---|---|---|---|
| Pandoc + LaTeX | Excellent typography | Heavy (TeX distribution, GBs) | Doesn't match the preview theme; Mermaid needs filters. |
| Bundled headless Chromium | Excellent CSS print | +150 MB per platform | Unacceptable for a "light" app. |
| **Paged.js + platform webview print-to-PDF** | Very good, matches preview exactly | **Zero** | Chosen. |
| System Chrome/Edge `--headless --print-to-pdf` | Excellent | Only if a browser is installed | Used as a fallback and on Linux. |

**Original M7 choice:** build one self-contained HTML page (theme CSS, embedded fonts and images),
paginate it with **Paged.js** (MIT) for running headers, `counter(page)`/`counter(pages)`,
`target-counter` TOC page numbers and break control, and print it with a headless
**Chromium-family browser driven over the DevTools protocol** (`Page.printToPDF`, after the page
signals that pagination and fonts are done).

**Current choice:** the same self-contained HTML is rendered by **Takumi** (MIT OR Apache-2.0),
natively in the desktop app and from a local WASM build in the browser demo. A visual review of all
PDF templates accepted it as the default. The original Paged.js path is retained for explicit comparison in the legacy Tauri shell only;
details and comparison images are in `docs/visual-reviews/takumi/`.
- **Windows:** Microsoft Edge, which ships with Windows 10/11, so **zero install**.
- **macOS / Linux:** any installed Chrome, Edge, Chromium or Brave. If none is found, the
  system print dialog opens ("Save as PDF"). Same layout and styles, but no running headers or
  page numbers.
- Why not `--print-to-pdf` with `--virtual-time-budget`: measured to print *before* Paged.js finishes,
  non-deterministically (7 pages one run, 2 the next, sometimes blank). Driving the DevTools protocol waits for a
  real readiness signal and was deterministic across repeated runs (about 1 s per document).
- Why not the webview's own print: WebKit has no `@page` margin boxes or `target-counter`, and
  per-OS native print-to-file paths are hard to make consistent.
- Pandoc is an **optional** extra (Word, OpenDocument, EPUB) offered only when `pandoc` is on PATH.

PDF options: page size (A4, Letter, A5, Legal), margins (narrow, normal, wide), title header (not
on the first page), "N / M" page numbers, table of contents with page numbers, H1 on a new page, and
manual breaks with a `---pagebreak---` line (also `\newpage`, `\pagebreak`, `<!-- pagebreak -->`).
HTML export is one self-contained file (fonts and images embedded; images optional). "Copy as HTML"
puts rich HTML on the clipboard.

**Custom themes:** `.css` files in `<app data>/themes/` (an example, *Sepia*, is created on first use).
A header comment names the theme (`@name`) and picks its base style (`@base paper|studio|typewriter`).
Theme CSS applies to the preview, live mode and every export, and theme files are re-read when the window regains focus.

## 7. Milestones

| | Scope |
|---|---|
| M1 | App shell, open/save, source mode, autosave, status bar |
| M2 | Preview, split view with accurate scroll sync, themes |
| M3 | File explorer, favorites, outline, persistent tabs |
| M4 | Live/hybrid mode |
| M5 | Command palette, context menus, selection bubble, customizable toolbar, slash commands |
| M6 | Spaces and multiple windows |
| M7 | HTML/PDF export, custom CSS |
| M8 | Spellcheck, embeds, math, Mermaid |
| M9 | Packaging, CI for 3 OSes, README with screenshots, CONTRIBUTING, LICENSE |

Each milestone lives on its own branch (`m1-shell`, `m2-preview`, …), each branched from the previous one.

## 8. Clean-room rule

Typora, Markdown Monster, Zettlr, and Obsidian may be used to study *behaviour* only. No code,
assets, or styles are copied. Zettlr is GPL. Third-party licenses are tracked in
`THIRD_PARTY_LICENSES.md`, and only permissive licenses (MIT, BSD, Apache-2.0, ISC, OFL for fonts) are allowed.
