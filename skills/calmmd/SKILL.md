---
name: calmmd
description: Work with notes in a running CalmMD editor through its local MCP server. Use for reading, editing, renaming, sorting, summarising and exporting Markdown documents while preserving CalmMD's live buffers, undo history and file history.
---

# CalmMD notes

Use this skill when a task involves documents that are open in CalmMD, or when the user asks to
read, edit, reorganise, summarise or export notes in a CalmMD folder.

CalmMD's MCP server is a bridge into the **live app**, not a generic filesystem server. Prefer its
tools over direct file writes: that keeps unsaved edits, autosave, drafts, conflicts, undo, file
history, tabs and favorites authoritative.

## Before you start

1. Confirm CalmMD is running, a folder is open, and the MCP server has been started.
2. Get the `url` and bearer token from the user or the connection string CalmMD copied to the
   clipboard. They persist between launches; **New MCP Token** replaces the token.
3. Discover the surface from the server rather than assuming a fixed list:
   - `tools/list`
   - `prompts/list`
   - `resources/list` and `resources/templates/list`
4. Call `calmmd_status` first. It reports the open root, open documents and active document.
5. Call `calmmd_windows` when more than one window or Space may be involved.

Do not write to the filesystem behind CalmMD's back. Do not start a second process to edit the same
files while CalmMD has them open.

## Choosing a tool

| Goal | Use |
|---|---|
| Inspect app/root/tabs | `calmmd_status` |
| Discover window targets | `calmmd_windows` |
| Find notes | `calmmd_list_documents`, `calmmd_search`, `calmmd_outline` |
| Read a note | `calmmd_read_document` (preferred over reading the file from disk) |
| Create notes/folders | `calmmd_create_document`, `calmmd_create_folder` |
| Rename or move notes | `calmmd_rename_document`, `calmmd_move_document` |
| Edit a note | `calmmd_write_document` |
| Undo / redo a Rust-backed edit | `calmmd_undo_document`, `calmmd_redo_document` |
| Remove a note | `calmmd_trash_document` — only with explicit user confirmation |
| Organise a large folder | `calmmd_triage_folder` + `calmmd_apply_organization` — plan first, apply second |
| Export | `calmmd_export_pdf`, `calmmd_export_html` |
| Summarise / make a guide | `calmmd_summarize`, `calmmd_follow_up_guide` prompts, then `calmmd_write_document` |
| Group notes into subfolders | `calmmd_organize_folder` prompt, then `calmmd_apply_organization` |

Read-only resources are also available:

- `calmmd://status`
- `calmmd://windows`
- `calmmd://document/<percent-encoded-path>`
- `calmmd://folder/<percent-encoded-path>`

Use tools for writes; resources cannot edit.

## Paths and routing

- Paths may be absolute or relative to the folder open in CalmMD. Prefer relative paths.
- `..` and paths outside the root are rejected. There is no sandbox escape.
- All tools except `calmmd_status` accept optional `windowLabel` and `spaceId`.
- With no target, the call goes to the window labelled `main`.
- Use `calmmd_windows` to find valid labels and Space ids before targeting another window.
- A `spaceId` must currently be held by a window; otherwise the call falls back to `main`.

## Editing safely

1. Read before writing. Use `calmmd_read_document` so unsaved buffer text is included.
2. Prefer `mode: "append"` or `"prepend"` for additive changes. Use `"replace"` only when the full
   intended text is known.
3. For a small targeted edit, compute the full new text from the live read and send it with
   `mode: "replace"`.
4. Set `flush: true` (the default) when the user expects the change on disk immediately.
5. After writing, re-read the document and confirm the result.
6. If a write fails because the document is in `conflict`, stop and ask the user to resolve the
   conflict in CalmMD. Do not overwrite.

Every MCP write is a CodeMirror transaction with `userEvent: 'input.mcp'`, so it is undoable, and
CalmMD snapshots the old and new text into file history.

## Renames and link safety

`calmmd_rename_document` does more than rename a file:

- open tabs, favorites and the history map are retargeted;
- relative Markdown links and `[[wiki]]` links in every note are rewritten;
- code blocks and inline code are deliberately left alone.

`calmmd_move_document` does the same for a move. Use it after creating the destination folder with
`calmmd_create_folder`, or let `calmmd_apply_organization` do both. After any rename or move, tell
the user which file changed and surface link-rewrite failures.

## Organising a large folder

There is no numeric sort tool. Use a two-phase, content-aware workflow:

1. Fetch the `calmmd_organize_folder` prompt for the folder, or call `calmmd_triage_folder`
   directly. The inventory contains live excerpts, headings, front matter, mtime and subfolders.
2. Let the client's model propose a JSON plan:
   `{ "moves": [{ "from", "toFolder", "newName?", "reason?" }] }`.
3. Call `calmmd_apply_organization` with `apply: false` to resolve paths, validate names and
   reject duplicate/occupied destinations.
4. Show the preview to the user, then call again with `apply: true` only after confirmation.

Application creates missing folders, moves and renames through `explorer`, and rewrites links in
every note. It is not transactional: report per-move failures. Do not invent moves for notes that
are already sensibly placed.

## Summaries and follow-up guides

The prompts do not run an LLM inside CalmMD. They inject the live note text into instructions for
the connected client's model. Fetch them with the MCP prompt methods (`prompts/list`, then
`prompts/get`), **not** with `tools/call`. They are not tool calls.

The note prompts take `path`; `calmmd_organize_folder` takes `folder`, `limit` and `excerptChars`.
All accept optional `windowLabel` and `spaceId`. Paths are resolved against the open folder,
including unsaved buffer text.

| Use | When | Expected output |
|---|---|---|
| `calmmd_summarize` | The user wants a condense/overview/recap of an existing note, or context for a longer task | Short Markdown; headings only if the note is long |
| `calmmd_follow_up_guide` | The user wants to turn a note into actionable next steps, a checklist, or a repeatable procedure | Markdown title, 3–6 numbered steps, short checklist; every step grounded in the note |
| `calmmd_organize_folder` | A large flat folder needs grouping into a handful of topic subfolders and clearer names | JSON move plan for `calmmd_apply_organization` |

Do not use a prompt for mechanical edits, renaming, moving or exporting — use the tools instead.
Do not use `calmmd_write_document` to invent content the prompts were meant to generate.

Recommended flow:

1. Read the note with `calmmd_read_document` if you need to understand it yourself.
2. Fetch the appropriate prompt with `prompts/get` and its `path` (plus routing fields if needed).
3. Let the client's model produce the Markdown from the returned instructions.
4. Show the generated text to the user and ask where to store it.
5. Save with `calmmd_write_document`, usually `mode: "append"`.
6. Re-read the note with `calmmd_read_document` to verify.

Never present generated text as if CalmMD generated it.

## Exporting

- Use `calmmd_export_html` for a standalone HTML copy.
- Use `calmmd_export_pdf` when the user asks for a PDF; it uses CalmMD's renderer and print engine.
- `out` is a destination path inside the open folder.
- `prefs` is merged over CalmMD's defaults; omitted fields keep their defaults.
- PDF export can fail if no Chromium/Edge browser is installed. Report that plainly; HTML still works.

`prefs` fields:

```json
{
  "pdf": {
    "pageSize": "A4",
    "margins": "normal",
    "header": true,
    "pageNumbers": true,
    "breakBeforeH1": false,
    "template": "standard"
  },
  "toc": false,
  "embedImages": true,
  "theme": null,
  "cover": false,
  "coverAuthor": ""
}
```

Valid values: `pageSize` = `A4`/`Letter`/`A5`/`Legal`; `margins` = `narrow`/`normal`/`wide`;
`template` = `standard`/`report`/`academic`/`minimal`; `theme` is a document style id or `null`.

For a cover page, pass `prefs.cover: true`, or pass a `cover` argument with any of `title`,
`subtitle`, `author` and `date`. Providing `cover` enables the cover even if `prefs.cover` is not
set. Omitted cover fields come from the note's front matter, first heading, remembered author and
today's date, exactly like the export dialog. PDF always embeds images.

## Destructive actions

- `calmmd_trash_document` requires `confirm: true`. Only pass it after the user explicitly agrees.
- The `confirm: true` argument is the confirmation; CalmMD does not show a second dialog for MCP calls.
- There is no folder-trash tool yet; do not emulate it by deleting files directly.

## Common mistakes to avoid

- Reading the file from disk when the user has unsaved edits in the buffer.
- Using raw `write` / `rename` tools instead of the CalmMD MCP tools.
- Applying a sort without showing the plan first.
- Assuming `windowLabel` values; call `calmmd_windows`.
- Assuming the bearer token changed after a restart; it persists until **New MCP Token**.
- Trying to write through a read-only resource URI.
- Expecting CalmMD to generate summaries itself.

## Quick examples

```text
User: "Organise my meeting notes into topics."
→ calmmd_status
→ prompts/get calmmd_organize_folder { folder: "meetings" }
→ generate JSON plan with the client model
→ calmmd_apply_organization { folder: "meetings", plan, apply: false }
→ show plan, get confirmation
→ calmmd_apply_organization { folder: "meetings", plan, apply: true }
```

```text
User: "Add a summary at the end of Project.md."
→ calmmd_read_document { path: "Project.md" }
→ prompts/get calmmd_summarize { path: "Project.md" }
→ generate Markdown with the client model
→ calmmd_write_document { path: "Project.md", content: "...", mode: "append" }
→ calmmd_read_document { path: "Project.md" }
```

```text
User: "Turn my meeting notes into a follow-up guide."
→ calmmd_read_document { path: "meetings/2026-07-28.md" }
→ prompts/get calmmd_follow_up_guide { path: "meetings/2026-07-28.md" }
→ generate Markdown with the client model
→ calmmd_write_document { path: "meetings/2026-07-28.md", content: "...", mode: "append" }
```

```text
User: "Work on the note in my Writing Space."
→ calmmd_windows
→ calmmd_read_document { path: "plan.md", spaceId: "writing" }
```

For connection details and troubleshooting, see `docs/MCP-CLIENT-SETUP.md` in the CalmMD repository.
