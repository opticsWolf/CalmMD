# GPUI ↔ Tauri visual parity pass

The two screenshots were taken from regular desktop windows at 1400×900 with the same five-document
fixture in `fixture/`. Both use the dark palette.

| Shell | Capture |
|---|---|
| Tauri | `tauri-dark.png` |
| GPUI Kit | `gpui-dark.png` |

## Fixed during this pass

- The GPUI `AppMenuBar` filled the entire window and left the dock zero pixels tall. It now lives in a
  fixed 28 px header, so the sidebar/editor/preview are visible.
- The first GPUI window used `window-2` while core state and MCP use `main`. It now uses `main`, so
  the restored folder and active note load on startup.
- `WorkspaceState::refresh_documents()` expected `{ documents: [...] }`, but core returns an array.
  The five fixture files now appear in the sidebar.
- The sidebar was a vertical stack of large utility buttons. It is now a compact Files/Outline/Search/
  History header, a search box, the folder name and a document list, with utilities in a footer.
- The editor gained a document tab and a small real-action toolbar (Save, Undo, Redo, Copy Markdown).
- Front matter renders as a quiet amber-ruled source block instead of Kit's description list.
- Body and mono font sizes were raised to 18 px / 20 px, so source wrapping and line density match
  the Tauri split view.
- A local title row plus Tauri's Space selector and favourites hint make the sidebar and chrome read
  as the same product.
- The default spell language is now the bundled US English dictionary, so misspellings get the same
  visible diagnostics as the incumbent shell.
- Preview padding was increased from 12 px to 28 px, matching Tauri's reading gutter.

## Still different

- The Tauri editor has full formatting buttons and syntax colours; GPUI has a smaller action set and
  Mordant/syntect highlighting.
- Tauri shows favourites, tabs, status details and richer sidebar views. GPUI shows the file list and
  search only.
- Preview parity is structural but Mermaid, KaTeX and custom CSS themes are still native
  approximations.
- Light-mode captures, richer toolbar controls and per-OS font fallback checks are still pending.

Captures can be reproduced with `scripts/capture-desktop.ps1` once both apps are open on the fixture.
