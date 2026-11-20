# Contributing to CalmMD

Thanks for helping. CalmMD aims to be a *calm* editor, so the best contributions often make
something simpler, faster or more reliable rather than adding UI.

## Before you start

- Read [SPEC.md](SPEC.md), especially the **design principles**. They override feature requests:
  calm UI, progressive disclosure, subtle branding, and the explicit non-goals (no git integration, AI chat,
  screen capture or plugin marketplace in v1).
- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the architecture, the rules that keep things from breaking (scroll sync,
  live mode, saving, export), and the debugging tips. Read [docs/PACKAGING.md](docs/PACKAGING.md)
  when changing the desktop build or release artifacts.
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
npm run app:dev      # GPUI Kit desktop app (Rust 1.95+; MSVC on Windows, Vulkan on Linux)
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

The workspace requires Rust 1.95 or newer. `apps/calmmd-gpui` is the default desktop binary;
`src-tauri` remains a one-release compatibility shell. Both share `crates/calmmd-core`; see section
11 of `docs/RUST-CORE-PLAN.md` before changing the split.

`cargo test -p calmmd-core renders_a_simple_pdf` runs the real Takumi PDF test. For a headless MCP
run, start `cargo run -p calmmd-core --bin calmmd-mcp -- --root <folder>` and then run
`npm run test:mcp` with the printed URL and token. CI runs the Rust tests, GPUI build, packaging and
MCP smoke on Windows, macOS and Linux.

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

1. Bump the workspace version in `Cargo.toml` and `package.json`.
2. Commit, then tag: `git tag v0.2.0 && git push --tags`.
3. CI builds the GPUI artifacts for every platform; follow `docs/PACKAGING.md` for signing and
   notarization before attaching them to a draft release.
4. Check the draft, write release notes, and publish.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
