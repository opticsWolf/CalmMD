# CalmMD MCP implementation

This document describes the embedded Model Context Protocol (MCP) support added to CalmMD,
what it can do today, and what is still open. It is a design record, not a user guide; for the
user-facing summary see [`README.md`](../README.md), for client setup see
[`MCP-CLIENT-SETUP.md`](MCP-CLIENT-SETUP.md), and for the app's overall structure see
[`ARCHITECTURE.md`](ARCHITECTURE.md). Agent workflow guidance lives in
[`../skills/calmmd/SKILL.md`](../skills/calmmd/SKILL.md).

---

## 1. Goal and scope

CalmMD can host a **local MCP server** so an external AI client can work on the documents that are
currently open in the app. The server works on the *live app state*, not a second filesystem implementation. All tools
call Rust services directly; the webview mirrors its buffers into those services and applies their
state-change events. Rust also owns the export assets, body preparation and standalone document
assembly, so MCP HTML/PDF tools do not need a running UI process.

- reads prefer the mirrored open buffer, so unsaved edits are visible;
- writes update Rust state, write atomically, snapshot history, and emit an event the editor applies
  as an undoable change;
- renames/moves update Rust state and history, rewrite links, and emit events that retarget the
  explorer and open tabs;
- PDF/HTML exports go through the existing renderer, so output matches the app.

CalmMD does **not** contain an LLM. The connected MCP client generates summaries and follow-up
guides (prompt templates are provided), then writes the result back with a tool.

The server is local-only, off by default, and has no account, cloud service or telemetry.

---

## 2. Status today

Implemented and building as of this document:

| Area | Status |
|---|---|
| Embedded Rust MCP server (`rmcp` 3.11.0, `2026-07-28` spec) | ✅ |
| Streamable HTTP over loopback with bearer token + `Origin` check | ✅ |
| Start/stop/status commands and menu/palette entries | ✅ |
| Persisted auto-start setting and status-bar indicator | ✅ |
| Browser-free headless exports in Rust | ✅ |
| Rust-owned workspace state for read/write/list/search/outline/status | ✅ |
| Rust file history and link-safe rename/move | ✅ |
| 18 live-state tools | ✅ |
| 3 prompt templates (summary, follow-up guide, folder organisation) | ✅ |
| 2 static resources + 2 resource templates | ✅ |
| Client setup guide and agent skill | ✅ |
| Multi-window/Space routing | ✅ |
| Link-safe rename, move and content-aware folder organisation | ✅ |
| Headless PDF/HTML export tools | ✅ |
| Unit tests for the bridge, links, origin check and settings | ✅ |

---

## 3. Architecture

```text
MCP client (Claude Desktop, Cursor, VS Code, …)
        │  MCP JSON-RPC / Streamable HTTP
        ▼
Rust MCP server                         crates/calmmd-core/src/mcp/
  mod.rs      server lifecycle, auth, commands
  bridge.rs   window routing (the old webview request bridge is retired)
  tools.rs    tools, prompts and read-only resources
        │  direct Rust calls
        ▼
Rust workspace                          crates/calmmd-core/src/core/workspace.rs
  rooted path sandbox, live buffers, history, link rewrites, file ops
        │  tauri event: calmmd://workspace
        ▼
Webview controllers
  documents.applyWorkspaceContent / explorer.externalRenamed / externalTrashed
        │
        │  tauri commands: core_workspace_sync / core_workspace_upsert_document
        ▼
Rust workspace mirror (user edits)

PDF/HTML export:
  tools.rs → core/export/body.rs → core/export/assemble.rs → Takumi/WASM-independent file write
```

### 3.1 Why the server lives in Rust

Tauri already owns the process, the window registry (`SpaceClaims`) and the OS capabilities. An
MCP server in Rust can therefore:

- hold a random loopback port in Tauri-managed state;
- route calls to the window holding a Space;
- expose lifecycle commands to the webview without a second runtime;
- route all tool calls, including export, without waiting on the view layer.

The webview deliberately has **no MCP SDK**. It mirrors Rust state changes and no longer answers
export requests. That
keeps protocol concerns in Rust and application concerns in TypeScript.

### 3.2 Transport and security

- Endpoint: `http://127.0.0.1:<random-port>/mcp`
- Transport: MCP Streamable HTTP (POST/GET, JSON or SSE responses)
- Authentication: `Authorization: Bearer <token>`
- Token: generated per launch with `uuid::Uuid::new_v4()`
- `Origin`: accepted only when absent/`null` or from `localhost`, `127.0.0.1`, or `::1`
- Binding: `127.0.0.1:0` only; never `0.0.0.0`
- Start triggers:
  - File → Start MCP Server
  - command palette: `Start MCP Server`
  - `--mcp` launch argument
  - `mcpAutostart` setting (off by default)

The server does not implement OAuth. For a loopback, single-user endpoint a per-launch bearer
token is sufficient and matches CalmMD's no-account privacy model.

### 3.3 Request flow

1. An MCP client calls a tool with arguments.
2. `tools.rs` resolves `windowLabel`/`spaceId` to a workspace key.
3. Read/write/file tools call `WorkspaceRegistry` directly. Path sandbox, live-buffer reads,
   atomic writes, history snapshots and link rewrites all run in Rust.
4. Mutations emit `calmmd://workspace` events; the webview applies them to its controllers. User
   edits flow back with `core_workspace_upsert_document`.
5. PDF/HTML tools resolve paths, read the live buffer, normalize preferences and build the cover in
   Rust (`core/export/prefs.rs`). `core/export/body.rs` renders Markdown, embeds local images and
   offline assets, then `assemble.rs` builds the standalone document. Takumi writes the PDF without
   a browser.

Tools return errors as `CallToolResult::error` with a text message. Prompts map failures to
`ErrorData::internal_error`.

---

## 4. Live-state integration points

### 4.1 Platform abstraction

`src/platform/types.ts` gained:

- `mcpStatus()`
- `mcpStart()`
- `mcpStop()`
- `onMcpRequest(handler)`
- `mcpRespond(response)`

`src/platform/tauri.ts` implements them with `invoke`/`listen`. The memory/browser platforms
return a disabled status and ignore requests, so the app still runs and tests in a browser.

### 4.2 Document writes

`createDocuments()` gained `setContentByPath(path, text, { mode, flush })`:

- opens the file as a tab if it is not already open;
- refuses to write a document in `conflict`;
- builds the next text with `replace` (default), `append`, or `prepend`;
- dispatches a minimal CodeMirror change with `userEvent: 'input.mcp'`, so it is undoable;
- touches the autosaver and optionally flushes it;
- calls `onExternalEdit` before applying, which App wires to file history.

That last point means an agent edit is reversible: the previous text and the resulting text are both
saved as versions.

### 4.3 Explorer operations

`createExplorer()` is the single entry point for create, rename and trash. MCP calls it rather than
raw Tauri commands, so existing validation and retargeting still apply:

- name validation and `.md` default extension;
- never overwrite on rename;
- favorites, expanded folders, root, open tabs and history map are rebased;
- `onRenamed` now awaits link rewriting.

### 4.4 Link rewriting

`rewriteLinks()` in `src/core/noteLinks.ts` retargets links that point at a renamed file:

- relative Markdown links, preserving anchors and titles;
- wiki links, preserving `#heading` and `|alias`;
- code fences, indented code and inline code are left untouched;
- cross-drive relative links are skipped because they cannot be expressed.

After a rename, App walks every Markdown file under the root. Open tabs are edited through
`docs.setContentByPath()`; closed files are read, rewritten and atomically written, with history
snapshots on both sides.

### 4.5 Export

MCP exports no longer call `src/app/exporter.ts`. `core/export/body.rs` takes the live text and
output path resolved by `tools.rs`, renders it with the same Mordant pipeline used by the app, and
embeds the vendored CSS/font assets. Takumi is the PDF backend; the browser/Paged.js path remains an
explicit comparison option in the Tauri UI only.

---

## 5. Tool reference

All tools except `calmmd_status` accept these optional routing fields:

| Field | Meaning |
|---|---|
| `windowLabel` | Run the call in this Tauri window label |
| `spaceId` | Resolve to the window holding this Space via `SpaceClaims` |

With neither, the call goes to `main`.

| Tool | Arguments | Notes |
|---|---|---|
| `calmmd_status` | — | Window label, window labels, root, open paths, active path |
| `calmmd_windows` | — | Window labels, titles and claimed Space ids for routing |
| `calmmd_list_documents` | `markdownOnly?`, `folder?` | Cached file list; `folder` filters recursively |
| `calmmd_read_document` | `path` | Live buffer for open tabs, disk otherwise; returns headings too |
| `calmmd_search` | `query`, `caseSensitive?`, `wholeWord?` | Delegates to `platform.searchFiles()` |
| `calmmd_outline` | `path` | Uses the shared heading scanner |
| `calmmd_create_document` | `folder?`, `name`, `text?` | Creates, opens, optionally writes initial text |
| `calmmd_create_folder` | `folder?`, `name` | Creates a folder inside the open folder |
| `calmmd_rename_document` | `path`, `name` | Retargets tabs, favorites, history, links |
| `calmmd_move_document` | `path`, `folder`, `name?` | Moves, optionally renames, retargets links |
| `calmmd_write_document` | `path`, `content` or `text`, `mode?`, `flush?` | Undoable, autosaved, snapshotted |
| `calmmd_undo_document` | `path`, routing fields | Restores the last Rust-owned change; updates the open view |
| `calmmd_redo_document` | `path`, routing fields | Reapplies the last undone change |
| `calmmd_trash_document` | `path`, `confirm` | `confirm: true` is the caller's confirmation; no webview modal |
| `calmmd_triage_folder` | `folder?`, `limit?`, `excerptChars?` | Live excerpts, headings, front matter, subfolders |
| `calmmd_apply_organization` | `folder?`, `plan`, `apply?` | Validates and applies a move/rename plan via CalmMD |
| `calmmd_export_pdf` | `path`, `out`, `prefs?`, `cover?` | Headless; returns the output path |
| `calmmd_export_html` | `path`, `out`, `prefs?`, `cover?` | Standalone HTML |

`prefs` is merged over `DEFAULT_EXPORT_PREFS`; PDF options use the same shape as the export dialog.
Providing `cover`, or `prefs.cover: true`, generates a cover page with defaults derived from the
note (front matter, first heading, today's date); an explicit `cover` object overrides those fields.

### 5.4 Export preferences

The Rust argument schema now describes the preference fields, so clients can discover them through
`tools/list` instead of seeing an untyped object.

```jsonc
{
  "pdf": {
    "pageSize": "A4",        // A4 | Letter | A5 | Legal
    "margins": "normal",     // narrow | normal | wide
    "header": true,
    "pageNumbers": true,
    "breakBeforeH1": false,
    "template": "standard"   // standard | report | academic | minimal
  },
  "toc": false,
  "embedImages": true,
  "theme": null,              // document style id, or null for the preview style
  "cover": false,
  "coverAuthor": ""
}
```

`cover` is a separate optional argument with `title`, `subtitle`, `author` and `date`. It is merged
over `coverFromDoc()`, the same defaulting used by the export dialog. PDF always embeds images,
regardless of `embedImages`.

### 5.1 Folder organisation workflow

`calmmd_sort_documents` was removed: numeric prefixes were a filename trick, not content-aware
reordering. The replacement is a two-phase triage flow:

1. `calmmd_triage_folder` inventories a folder's Markdown files using live buffers. Each note
   includes a short excerpt, headings, front matter, mtime and an `open` flag; existing subfolders
   are listed too.
2. The `calmmd_organize_folder` prompt asks the connected client's model to propose a JSON plan of
   `{ from, toFolder, newName?, reason? }` moves. CalmMD has no built-in LLM.
3. `calmmd_apply_organization` validates paths, rejects duplicate/occupied destinations, and
   optionally creates folders, moves files and renames them through `explorer`.

With `apply: false` the tool returns a preview. With `apply: true` each move runs in the Rust
workspace service, which rewrites links and emits rename/content events for the webview. Failures
are reported per move; the operation is not transactional.

### 5.2 Prompts

| Prompt | Arguments | Result |
|---|---|---|
| `calmmd_summarize` | `path`, routing fields | Instruction containing the live note text, asking for a short Markdown summary |
| `calmmd_follow_up_guide` | `path`, routing fields | Instruction asking for a title, 3–6 numbered steps and a checklist |
| `calmmd_organize_folder` | `folder?`, `limit?`, `excerptChars?`, routing fields | Live folder inventory plus instructions to propose a JSON organisation plan |

These are prompts, not tools: the server supplies context and instructions; the connected client's
model produces the text. Clients list them with `prompts/list` and fetch them with `prompts/get`,
not `tools/call`. All accept optional `windowLabel`/`spaceId`; the note prompts take `path`, while
`calmmd_organize_folder` takes `folder`, `limit` and `excerptChars`.

Use `calmmd_summarize` for a condense/overview/recap. Use `calmmd_follow_up_guide` when the note
should become actionable next steps or a repeatable procedure. Persist their generated text with
`calmmd_write_document`, usually with `mode: "append"`. Use `calmmd_organize_folder` for a large
flat folder, then apply its plan with `calmmd_apply_organization`.

### 5.3 Resources

| URI | Content |
|---|---|
| `calmmd://status` | JSON status of the main window |
| `calmmd://windows` | JSON list of window labels, titles and Space ids |
| `calmmd://document/{path}` | Live Markdown of a note, including unsaved edits |
| `calmmd://folder/{path}` | JSON list of Markdown files in a folder |

Paths are percent-encoded. `windowLabel` and `spaceId` can be appended as query parameters, e.g.
`calmmd://document/notes%2Fplan.md?spaceId=writing`. Resources are read-only; use tools for writes.

Clients discover them through `resources/list` and `resources/templates/list`.

---

## 6. Configuration and UI

- `mcpAutostart` lives in `src/app/settings.ts`, default `false`.
- File menu: Start MCP Server, Stop MCP Server, Auto-start MCP Server (checked state from settings).
- Command palette: same three commands under the `MCP` category.
- Starting copies `url · token: …` to the clipboard.
- A small `MCP` marker appears in the status bar while the server is enabled.

When `--mcp` is used, startup begins before the webview has loaded settings; that is safe because
`mcpStart()` is idempotent and returns the already-running status.

---

## 7. Files changed

Rust:

- `crates/calmmd-core/src/mcp/mod.rs`, `bridge.rs`, `tools.rs`
- `crates/calmmd-core/src/core/workspace.rs`, `history.rs`, `links.rs`, `paths.rs` — live workspace services
- `src-tauri/src/lib.rs` — module, setup, commands, `--mcp`
- `src-tauri/Cargo.toml` / `Cargo.lock` — `rmcp`, `axum`, `tokio`, `uuid`, `schemars`, `sha2`; MSRV 1.91

TypeScript:

- `src/mcp/types.ts` — shared wire shapes
- `src/platform/{types,tauri,memory}.ts` — platform methods
- `src/app/mcpBridge.ts` — dispatcher and listener installation
- `src/app/documents.ts` — `setContentByPath()` and `onExternalEdit`
- `src/app/exporter.ts` — headless export methods
- `src/app/explorer.ts` — awaited `onRenamed`
- `src/core/noteLinks.ts` — `rewriteLinks()`
- `src/app/settings.ts` — `mcpAutostart`
- `src/App.tsx`, `src/app/appMenus.ts`, `src/styles/app.css` — wiring, commands, menu, indicator

Tests/docs:

- `src/app/mcpBridge.test.ts`
- `src/core/noteLinks.test.ts`
- `src/app/settings.test.ts`
- Rust `mcp::tests::only_local_origins_pass`
- `docs/MCP.md` (this file), `docs/MCP-CLIENT-SETUP.md`, `docs/ARCHITECTURE.md`, `README.md`,
  `THIRD_PARTY_LICENSES.md`
- `skills/calmmd/SKILL.md` and `.claude/skills/calmmd/SKILL.md` (identical copies; keep in sync)

---

## 8. Verification

Commands used:

```sh
npm run typecheck
npm test
npm run build
cd src-tauri && cargo check --all-targets
cd src-tauri && cargo test mcp::
```

At the time of writing: TypeScript typecheck, build and 387 tests pass; Rust check and the three MCP
unit tests pass. Full `cargo test` has two pre-existing spellchecker failures on this machine
because no `en-US` OS dictionary is installed; they are unrelated to MCP.

---

## 9. Gaps and open topics

### 9.1 Transport and client compatibility

- **No stdio transport.** Some clients only launch stdio servers. A small sidecar or a
  `--mcp-stdio` mode that forwards to the running app would help.
- **No persisted port/token.** They change every launch. Clients must be reconfigured, or the
  start command's clipboard payload must be used each time.
- **Resources are read-only and flat.** There is no resource for a whole Space, no subscriptions,
  and no resource templates with variable routing fields beyond query parameters.
- **No progress/cancellation notifications** for long exports or large sorts.
- **No session resumption** across app restarts.

### 9.2 Security and permissions

- **Path scoping lives in the Rust workspace.** PDF/HTML export resolves paths and builds the whole
  document in Rust, with no webview round trip.
- **Explicit Tauri capabilities.** `core:default` was replaced with only the event, menu and
  window permissions the webview uses. `opener:allow-open-path` remains broad for user-initiated
  local links and export reveals.
- **No per-root or per-tool permissions.** Today, starting the server grants all 18 tools for the
  open folder. A settings panel with read/write/export/trash scopes is still open.
- **No audit log.** File history captures content changes, but not which MCP client or tool made
  them.
- **No approval UI for writes.** Edits are undoable and snapshotted, but the AI client does not get
  an interactive diff for confirmation. MRTR/elicitation could be used for that.
- **Static bearer token only.** Deliberate for local use; no OAuth, no client identity beyond the
  token.

### 9.3 Concurrency and routing

- **No global operation queue.** Two concurrent `calmmd_write_document` calls on the same file can
  interleave dispatch/flush. The JS event loop serialises dispatch, but a explicit queue would make
  ordering and error handling deterministic.
- **`spaceId` resolution can return no window** if a Space is not claimed. The tool then falls back
  to `main` via `McpBridge::call(None, …)`; this should probably become an explicit error.
- **`calmmd_status` has no routing fields**, so it always reports main's state. `calmmd_windows`
  lists labels and Space ids, but not each window's root; asking every window for status would need
  a short per-window timeout.
- **No cross-window awareness in the status bar** beyond the local `MCP` marker.

### 9.4 Document operations

- **No folder trash tool.** Folders can be created, but removing a folder still needs Explorer or
  the OS file manager.
- **Link rewriting is partial.**
  - Image links (`![alt](...)`) are not rewritten by `rewriteLinks()`.
  - Links inside code are intentionally skipped.
  - External renames made in another app are not detected as renames; CalmMD sees a delete + add.
  - Absolute links across drives are skipped.
- **Organisation is client-model-driven.** CalmMD supplies the inventory and applies a validated
  plan; it does not decide the topics itself.
- **No duplicate-content detection.** Near-identical notes are only surfaced through excerpts and
  front matter; the client model must notice them.
- **Non-Markdown files can be listed but not read or exported** through MCP.

### 9.5 Product decisions

- `SPEC.md` still lists AI chat as a v1 non-goal. The current design respects that by keeping the
  model in the MCP client, but any in-app generation feature needs an explicit product decision.
- There is no settings UI panel yet; auto-start is only a command/menu toggle.
- No documentation yet in the app's Help dialog about connecting a client.
- No decision has been made about shipping this enabled in release builds or behind a build flag.

### 9.6 Suggested next steps

1. Add a Rust-side root allowlist and per-tool permission tiers.
2. Add image-link rewriting and bulk undo for organisation plans.
3. Add a serialised operation queue and explicit `spaceId` resolution errors.
4. Add a stdio shim for client compatibility.
5. Add a small Settings → MCP panel with enable, auto-start, scopes and connection details.
6. Add an approval-diff flow for `write_document` using MCP elicitation/MRTR.
7. Add `calmmd://space/{id}` resources and per-window status resources.
