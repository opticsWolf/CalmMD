//! Capability seams for GPUI. UI views never call platform APIs directly.

use gpui_kit::{App, ClipboardItem};
use std::path::{Path, PathBuf};

/// Desktop capabilities the shell needs. Each method has a tiny, testable surface.
#[allow(dead_code)]
pub trait Platform: Send + Sync {
    fn open_folder(&self) -> Option<PathBuf>;
    fn open_file(&self) -> Option<PathBuf>;
    fn open_image(&self) -> Option<PathBuf> { None }
    fn save_file(&self, default_name: &str) -> Option<PathBuf>;
    fn reveal(&self, path: &Path) -> Result<(), String>;
    fn open_url(&self, url: &str, cx: &mut App);
    fn copy(&self, text: &str, cx: &mut App);
}

/// Native implementation backed by `rfd` and OS reveal commands.
#[allow(dead_code)]
pub struct NativePlatform;

impl Platform for NativePlatform {
    fn open_folder(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    fn open_file(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "mdx", "txt"]).pick_file()
    }

    fn open_image(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"])
            .pick_file()
    }

    fn save_file(&self, default_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_file_name(default_name)
            .save_file()
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg("/select,")
                .arg(path)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            let target = path.parent().unwrap_or(path);
            std::process::Command::new("xdg-open")
                .arg(target)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        if safe_external_url(url) {
            cx.open_url(url);
        }
    }

    fn copy(&self, text: &str, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    }
}

/// Only hand browser-safe schemes to the OS. Markdown links are resolved in the core first, so
/// `file:` and `javascript:` URLs here are unexpected and are ignored.
pub fn safe_external_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

/// In-memory platform for tests and headless service checks.
#[allow(dead_code)]
#[derive(Default)]
pub struct MemoryPlatform;

impl Platform for MemoryPlatform {
    fn open_folder(&self) -> Option<PathBuf> {
        None
    }

    fn open_file(&self) -> Option<PathBuf> { None }

    fn save_file(&self, _default_name: &str) -> Option<PathBuf> {
        None
    }

    fn reveal(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    fn open_url(&self, _url: &str, _cx: &mut App) {}

    fn copy(&self, _text: &str, _cx: &mut App) {}
}

#[cfg(test)]
mod tests {
    use super::safe_external_url;

    #[test]
    fn only_safe_external_schemes_are_opened() {
        assert!(safe_external_url("https://example.test/note"));
        assert!(safe_external_url("mailto:ada@example.test"));
        assert!(!safe_external_url("javascript:alert(1)"));
        assert!(!safe_external_url("file:///C:/secret.txt"));
    }
}
