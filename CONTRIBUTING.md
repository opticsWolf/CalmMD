# Contributing to CalmMD

Thanks for helping. CalmMD aims to be a *calm* editor, so the best contributions often make
something simpler, faster or more reliable rather than adding UI.

## Before you start

- Read [SPEC.md](SPEC.md), especially the **design principles**. They override feature requests:
  calm UI, progressive disclosure, subtle branding, and the explicit non-goals (no git integration, AI chat,
  screen capture or plugin marketplace in v1).
- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the architecture, the rules that keep things from breaking (scroll sync,
  live mode, saving, export), and the debugging tips.
- For work on Rust-side rendering, spellchecking, PDF generation or the GPUI Kit shell migration, read
  [docs/RUST-CORE-PLAN.md](docs/RUST-CORE-PLAN.md). Section 11 is the active migration plan; do not
  start a UI rewrite without agreeing a gate there first. For the local MCP server read [docs/MCP.md](docs/MCP.md),
  [docs/MCP-CLIENT-SETUP.md](docs/MCP-CLIENT-SETUP.md) and [skills/calmmd/SKILL.md](skills/calmmd/SKILL.md).
  The MCP bridge exposes live app state to an external client; it is not an in-app AI chat and has no built-in LLM.
- For anything larger than a bug fix, open an issue first so we can agree on the approach.

## Setup

```sh
npm install
npm run dev          # browser build at http://localhost:1420 (no Rust needed)
npm run app:dev      # current desktop app (needs Rust + Tauri prerequisites until the GPUI cutover)
```

Add `?demo` to the browser URL for a showcase document. Options: `&mode=split|live|source|preview`,
`&scheme=dark`, `&theme=studio`, `&sidebar=outline|none`, `&palette=>query`.

## Checks (run before every pull request)

```sh
npm run typecheck
npm test             # Vitest unit tests
npm run test:rust    # cargo test
npm run build
```

The current Tauri desktop build requires Rust 1.91 or newer (matching `src-tauri/Cargo.toml`).
The GPUI Kit migration raises the MSRV to 1.92; see section 11 of `docs/RUST-CORE-PLAN.md`.

`cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored` also runs the real PDF test
(it needs Chrome or Edge). CI runs all of this on Windows, macOS and Linux.

## How the code is organised

- **Pure logic lives in `src/core`, `src/editor/format.ts`, `src/editor/table.ts`, `src/sync/scrollMap.ts`
  and `src/editor/live/analyze.ts`**, with tests next to it. If you change behaviour there, add a test.
- **Every user-facing action is a command** in the registry. Most commands live in
  `src/app/appCommands.ts`; commands that need window-local state (exports, MCP server status) are
  registered from `src/App.tsx`. Then it's in the palette, can have a shortcut, and can be put in
  the toolbar or a context menu. Don't add buttons that bypass the registry.
- The UI never calls Tauri directly. It goes through the `Platform` interface (`src/platform`), which also
  has an in-memory implementation for the browser build and tests.
- Colours, fonts and spacing are CSS variables (`src/styles/tokens.css`). Don't hard-code colours.

## Style

- TypeScript strict. Prefer small, named functions with a short comment saying *why*.
- UI copy is short, calm and plain: "Saved", not "Your document has been saved successfully!"
- Keep dependencies few and **permissively licensed** (MIT, BSD, Apache-2.0, ISC; OFL for fonts). Add new
  runtime dependencies to [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
- Windows note: don't rewrite source files with Windows PowerShell 5.1's `Get-Content` / `Set-Content`.
  It corrupts UTF-8 and adds a BOM.

## Clean-room rule

You may *use* Typora, Markdown Monster, Zettlr and Obsidian to understand behaviour. **Do not copy code,
assets or styles from them.** Zettlr is GPL, and the others are proprietary. Contributions that look
derived from them will be declined.

## Commits and pull requests

- Commit subjects in the imperative mood, under ~72 characters ("Keep embeds playing across re-renders").
- Explain the *why* in the body, especially for behaviour changes.
- Include screenshots or a short clip for visible UI changes, in light and dark if colours changed.

## Releasing (maintainers)

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Commit, then tag: `git tag v0.2.0 && git push --tags`.
3. The *Release* workflow builds installers for every platform and attaches them to a draft release.
4. Check the draft, write release notes, and publish.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
