# Packaging the GPUI Kit app

The default desktop binary is `calmmd-gpui`. The Tauri shell remains available for one compatibility
release through `npm run app:dev:legacy` and `npm run app:build:legacy`.

## Local build

```sh
npm run app:build       # cargo build --release -p calmmd-gpui
npm run app:package     # stages and packages the release binary
```

`scripts/package-gpui.mjs` reads identity and resources from `apps/calmmd-gpui/app.json` and produces:

| Platform | Artifacts |
|---|---|
| Windows | ZIP containing `calmmd-gpui.exe`, an `install.ps1` shortcut installer and resources |
| macOS | `CalmMD.app` tar.gz, plus a DMG when `hdiutil` is available |
| Linux | tar.gz, plus a `.deb` when `dpkg-deb` is available |

Each package includes `VERSION.txt`, `LICENSE`, `THIRD_PARTY_LICENSES.md`, `README.md` and
`PLATFORM-LIMITS.md`. Export CSS/font assets are compiled into `calmmd-core`; no webview runtime or
npm installation is needed at runtime.

## Identity and resources

- Bundle id: `app.calmmd.editor`
- Icons: `apps/calmmd-gpui/resources/icons`
- macOS minimum version: 15.0 (GPUI Kit requirement)
- Resource inventory: `apps/calmmd-gpui/resources/manifest.json`

## Signing and notarization

Unsigned builds are useful for development but are blocked by Gatekeeper and SmartScreen. A release
must:

1. Sign the Windows executable with an EV/OV code-signing certificate, then sign the installer or ZIP
   manifest as appropriate.
2. Sign the macOS `.app` with a Developer ID Application certificate, notarize the DMG with
   `xcrun notarytool`, and staple the ticket with `xcrun stapler`.
3. Sign the Linux `.deb` with `dpkg-sig`/`debsign` if the target repository requires it.

Signing credentials are never stored in the repository. Add them as CI secrets and run the signing
step only on trusted release tags.

## Release tests

For every artifact:

- [ ] install on a clean machine and launch without developer tooling;
- [ ] open a folder, edit a note and confirm autosave;
- [ ] export HTML and PDF offline, including a note with a local image;
- [ ] start MCP with `calmmd-gpui --mcp` and run `npm run test:mcp`;
- [ ] confirm a second launch forwards a Markdown file to the running instance;
- [ ] check light/dark previews and Literata/JetBrains Mono fallback.

CI builds these artifacts on Windows, macOS and Linux and runs the headless MCP smoke on each.
Manual checks are still required before publishing a release.
