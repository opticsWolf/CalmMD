# Tauri controls in GPUI

Audited against `src/app/appMenus.ts`, `src/app/appCommands.ts`, and `src/ui/`.

| Tauri surface | GPUI implementation | Remaining differences |
| --- | --- | --- |
| File menu and tabs | New and open document/window, open folder/parent/recent, clear recent folders, close folder/tab, reopen tab, save/copy, reveal, version history, MCP start/stop, quit | Untitled drafts, non-Markdown tabs, middle-click and tab context actions, share bundles, Pandoc formats, MCP autostart |
| Spaces menu and sidebar button | Save, open, rename via Save Space, close, remove with confirmation; saved records use core Space storage | Layout and cursor restoration, active Space restoration after app restart, continuous updates to saved Spaces |
| Edit menu | Undo/redo, clipboard operations, select all, find, folder search, Go to Line, Copy HTML | Rich paste and Tauri's editor context actions |
| View menu and status controls | Source/Split/Preview, sidebar and toolbar visibility, search/outline/history, zoom, light/dark palette selection, match current OS appearance | Live editor, synchronized scrolling, Tauri palettes and document styles, font/width settings, continuous OS appearance following, full status controls |
| Insert menu and toolbar | Link/image prompts, image file picker, configurable asset directory, table prompt, code, math, Mermaid source, footnote, horizontal rule | Pasted and dropped image import, graphical Excalidraw editing, Mermaid rendering, full KaTeX |
| Format menu | Inline styles, default highlight, heading levels, lists, quote, table row/column insertion and deletion, alignment, table formatting | Color swatches and custom colors, clear formatting, paragraph command |
| Toolbar | Shared editor actions, configurable button order and visibility, separators, reset, persisted layout | Selection formatting bubble, color swatch dropdowns |
| Tools and Help | Searchable command/file/heading pickers, spelling toggle and language picker, export, welcome, Markdown guide, keyboard shortcuts, About | Dictionary download, slash commands, update checks and notices, full Tauri guide content |
| Sidebar | File tree, search, outline, history, clickable line navigation | Favourites, file management context menus, non-Markdown quick view, backlink view, version comparison |
| Export and confirmation dialogs | Native file pickers; HTML and PDF options for contents, cover, image embedding, and PDF layout; version restore and Space removal confirmations | Export preview, saved export preferences, Tauri's cover metadata fields |
| Conflict and recovery | GPUI conflict banner, autosave, file-backed draft recovery | Different presentation from Tauri |
