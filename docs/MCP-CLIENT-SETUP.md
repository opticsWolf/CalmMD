# Connect an MCP client to CalmMD

CalmMD exposes a **local, opt-in MCP server**. This guide shows how to connect a client and verify
that it can discover the tools and prompts. For the implementation details see [`MCP.md`](MCP.md).

> The server is off by default. It listens only on `127.0.0.1`. The port and bearer token are
> remembered between launches so a client is configured once; treat the connection string like a
> password and rotate it with **File → New MCP Token**.

---

## 1. Start the server

1. Open a folder in CalmMD.
2. Choose **File → Start MCP Server** (or run **Start MCP Server** from the command palette, or
   launch the app with `--mcp`).
3. CalmMD copies a string like this to the clipboard:

   ```text
   http://127.0.0.1:53123/mcp · token: 8c1f4e3a-...
   ```

   Keep the app running. The URL and token survive restarts; only **New MCP Token** replaces
   them.

Optionally enable **File → Auto-start MCP Server** so this happens on future launches.

### Surface at a glance

| Capability | Count | Examples |
|---|---|---|
| Tools | 18 | `calmmd_status`, `calmmd_triage_folder`, `calmmd_write_document`, `calmmd_export_pdf` |
| Prompts | 3 | `calmmd_summarize`, `calmmd_follow_up_guide`, `calmmd_organize_folder` |
| Static resources | 2 | `calmmd://status`, `calmmd://windows` |
| Resource templates | 2 | `calmmd://document/{path}`, `calmmd://folder/{path}` |

A conforming client discovers all of these through `tools/list`, `prompts/list`, `resources/list`,
and `resources/templates/list`.

---

## 2. Portable configuration

Most current clients accept the portable `mcpServers` shape. Replace `PORT` and `TOKEN` with the
values from CalmMD's clipboard.

```json
{
  "mcpServers": {
    "calmmd": {
      "type": "http",
      "url": "http://127.0.0.1:PORT/mcp",
      "headers": {
        "Authorization": "Bearer TOKEN"
      }
    }
  }
}
```

Prefer an environment variable over a literal token in a shared config:

```json
{
  "mcpServers": {
    "calmmd": {
      "type": "http",
      "url": "http://127.0.0.1:PORT/mcp",
      "headers": {
        "Authorization": "Bearer ${env:CALMD_MCP_TOKEN}"
      },
      "tools": ["*"]
    }
  }
}
```

> The port and token persist in CalmMD's local state, so one entry keeps working across restarts.
> If the remembered port is taken by another process, CalmMD falls back to a free port; start the
> server again (or use **New MCP Token**) to copy the updated string.

---

## 3. Client-specific examples

### Claude Code

Add the server from a terminal:

```sh
claude mcp add --transport http --header "Authorization: Bearer TOKEN" calmmd http://127.0.0.1:PORT/mcp
claude mcp list
```

Then start `claude` and use `/mcp` to check that `calmmd` is connected.

For a reusable project entry, put this in `.mcp.json`:

```json
{
  "mcpServers": {
    "calmmd": {
      "type": "http",
      "url": "http://127.0.0.1:PORT/mcp",
      "headers": { "Authorization": "Bearer ${env:CALMD_MCP_TOKEN}" }
    }
  }
}
```

Use `--scope user` with `claude mcp add` to register it for all projects instead of the current
one. Claude Code reads `.mcp.json` at startup; approve the project server the first time it appears.

### VS Code / GitHub Copilot

Use the Copilot harness configuration file:

- all workspaces: `~/.copilot/mcp-config.json` (or `$COPILOT_HOME/mcp-config.json`)
- share with the workspace: `.mcp.json` at the workspace root

```json
{
  "mcpServers": {
    "calmmd": {
      "type": "http",
      "url": "http://127.0.0.1:PORT/mcp",
      "headers": {
        "Authorization": "Bearer ${env:CALMD_MCP_TOKEN}"
      },
      "tools": ["*"]
    }
  }
}
```

You can also run **MCP: Add Server** in the Command Palette and choose **Global → Copilot Global**.
Choose HTTP as the transport and enter the URL and header.

### Cursor

Use `~/.cursor/mcp.json` for a global server or `.cursor/mcp.json` for a project server:

```json
{
  "mcpServers": {
    "calmmd": {
      "url": "http://127.0.0.1:PORT/mcp",
      "headers": {
        "Authorization": "Bearer ${env:CALMD_MCP_TOKEN}"
      }
    }
  }
}
```

Cursor supports `url`, `headers`, and `${env:NAME}` interpolation for remote HTTP servers. It also
supports tools, prompts and resources, so the full CalmMD surface should appear in **Customize → MCP**.

### Claude Desktop chat app

Claude Desktop chat app configurations historically centered on stdio servers. If your version
supports remote HTTP servers, use the portable JSON above in `claude_desktop_config.json`. If it
only offers stdio, use Claude Code's `claude mcp add-from-claude-desktop` to copy an existing
configuration, or wait for the CalmMD stdio shim (see [open topics](MCP.md#91-transport-and-client-compatibility)).

---

## 4. Verify discovery

Ask the client to list MCP servers, or use its MCP panel. You should see:

- server name `calmmd`
- 18 tools, including `calmmd_status`, `calmmd_windows`, `calmmd_undo_document` and `calmmd_triage_folder`
- 3 prompts: `calmmd_summarize`, `calmmd_follow_up_guide`, `calmmd_organize_folder`
- 2 resources and 2 resource templates

Good first calls:

1. `calmmd_status` — confirms the folder is open and shows the active document.
2. `calmmd_windows` — shows valid `windowLabel` and `spaceId` values for routing.
3. `calmmd_list_documents` — confirms the path sandbox and live app bridge are working.

Prompts are discovered with `prompts/list` and fetched with `prompts/get`; they are not tool calls.
All take optional `windowLabel`/`spaceId`; the note prompts take `path`, while organisation takes
`folder`, `limit` and `excerptChars`.

| Prompt | Use when | Result shape |
|---|---|---|
| `calmmd_summarize` | The user wants a condense/overview/recap | Short Markdown; headings only for long notes |
| `calmmd_follow_up_guide` | The user wants actionable next steps or a checklist | Title, 3–6 numbered steps, short checklist |
| `calmmd_organize_folder` | A large flat folder needs grouping into subfolders and clearer names | JSON move plan for `calmmd_apply_organization` |

A prompt-driven flow looks like this:

1. Fetch `calmmd_summarize` or `calmmd_follow_up_guide` for a note with `prompts/get`.
2. The client's model produces Markdown from the returned instructions.
3. Show the generated text to the user and ask where to store it.
4. Call `calmmd_write_document` with `mode: "append"` to save it back.
5. Re-read the note to verify.

CalmMD itself has no LLM; generation always happens in the connected client.

For folder organisation:

1. Fetch `calmmd_organize_folder` for the folder with `prompts/get`.
2. The client's model returns a JSON plan of `{ from, toFolder, newName?, reason? }` moves.
3. Call `calmmd_apply_organization` with `apply: false` to preview and validate it.
4. Show the preview, then call again with `apply: true` to create folders, move files and rename
   ambiguous notes through CalmMD.

---

## 5. Resource URIs

Read-only resources are available for clients that support MCP resources:

| URI | Content |
|---|---|
| `calmmd://status` | JSON status of the main window |
| `calmmd://windows` | JSON list of windows, labels and Space ids |
| `calmmd://document/<path>` | Live Markdown of a note, including unsaved edits |
| `calmmd://folder/<path>` | JSON list of Markdown files in a folder |

Percent-encode the path. For example:

```text
calmmd://document/notes%2Fproject%20plan.md
calmmd://folder/notes%2Fprojects
```

Routing fields can be appended as query parameters:

```text
calmmd://document/notes%2Fplan.md?spaceId=writing
calmmd://document/notes%2Fplan.md?windowLabel=space-2
```

Those resources are read-only. Use tools for writes.

---

## 6. Export options

The export tools expose typed `prefs` fields through `tools/list`:

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

For a cover page, pass `prefs.cover: true` or a `cover` object with `title`, `subtitle`, `author`
and/or `date`. Omitted cover fields are filled from the note's front matter, first heading,
remembered author and today's date.

## 7. Troubleshooting

| Symptom | Check |
|---|---|
| `401` / “Needs authentication” | The token was rotated; use **New MCP Token** and copy the new string |
| `404` at `/mcp` | The URL must include `/mcp`; re-check the port if it had to rebind |
| Connection refused | CalmMD was closed, the server was stopped, or the app is running in browser mode |
| `Open a folder in CalmMD first` | MCP paths are resolved against the open folder's root |
| `That path is outside the open folder` | Use a relative path inside the root, or fix the `windowLabel`/`spaceId` target |
| No tools appear | Check the client's MCP panel and approve the server; verify `tools/list` |
| PDF export fails | A Chromium/Edge print engine is required; HTML export works without one |
| Client only supports stdio | CalmMD currently offers Streamable HTTP only |

Never commit a populated `Authorization` header to a public repository. Use an environment
variable or a personal config file.

---

## 8. Agent skill

The repository contains a ready-to-use agent skill at `skills/calmmd/SKILL.md`, plus an identical
copy at `.claude/skills/calmmd/SKILL.md` for Claude Code. It teaches the client when to use each
tool, why writes must go through CalmMD, how to do a two-phase sort, and how to save generated text.

## 9. Related documents

- [`MCP.md`](MCP.md) — implementation record, tool reference, architecture, gaps and open topics
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — how the live-state bridge fits into CalmMD
- [`../skills/calmmd/SKILL.md`](../skills/calmmd/SKILL.md) — agent workflow guidance for using the tools
