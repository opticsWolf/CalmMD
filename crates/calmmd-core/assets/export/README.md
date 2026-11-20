# Self-contained export assets

These files are the Rust core's offline copy of the assets that the Tauri webview used to load
through Vite. They let MCP build standalone HTML and PDF exports without a running UI, a browser,
or network access.

- `tokens.css` and `preview.css` come from `src/styles/`.
- `fonts/*.woff2` are the variable fonts from Fontsource packages (SIL Open Font License 1.1).
- `katex/` is KaTeX's stylesheet and woff2 fonts (MIT). The Rust asset loader inlines each font
  as a data URL, as `src/export/assets.ts` did.
- `paged.polyfill.min.js` is the explicit browser/Paged.js fallback (MIT).

When updating a font or stylesheet, copy the same version from `node_modules` and `src/styles`, then
run the export snapshot/visual checks.
