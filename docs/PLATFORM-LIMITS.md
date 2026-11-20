# GPUI Kit platform limits

This document records the platform boundary for the GPUI preview shell. The Tauri shell remains
the shipped binary until the cutover gate.

## Menus

- Windows and macOS use native OS menus through GPUI's application menu bar.
- Linux receives the same menu model; GPUI/Kit draws it as an in-window `AppMenuBar` and popup menus
  because there is no native application menu on the target Linux desktops.
- The in-app context menu uses GPUI Kit `NativeMenu`. On Linux it falls back to Kit's drawn
  `PopupMenu`, which is clipped to the window bounds.

## Files and URLs

- Folder and save dialogs use `rfd`.
- Reveal-in-folder uses `explorer /select,`, `open -R` or `xdg-open` depending on the OS.
- External links are limited to `http:`, `https:` and `mailto:`. Local Markdown links are resolved
  by `calmmd-core` before reaching the platform seam.
- Drag-and-drop of folders and `.md` files opens them through the same core workspace service as
  launch paths.

## Single instance and launch

- `instance.lock` in the data directory is held exclusively by one process.
- A later launch writes its paths to `launch-queue.json` and exits. The running instance polls the
  queue once per second and opens the queued folder or notes.
- The lock is advisory for the local user profile. It is not a cross-user or network lock.

## WebView fallback

- `gpui-webview` is not a dependency of the preview. It remains an optional future fallback for
  browser-only diagram comparisons.
- Linux Wayland is not supported by `gpui-webview`; the native preview must remain sufficient
  there.
- Paged.js/Chromium export remains an explicit comparison path in the legacy Tauri shell. The GPUI
  core export path uses Takumi by default and does not launch a browser.
