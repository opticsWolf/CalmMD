//! Tauri file I/O commands. Atomic writes and filesystem helpers live in `calmmd-core`.

use calmmd_core::fsops as core;
pub use calmmd_core::fsops::{describe, write_atomic};
use tauri::AppHandle;

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<core::ReadResult, String> {
    core::read_text_file(path)
}

#[tauri::command]
pub async fn write_text_file(path: String, text: String) -> Result<core::WriteResult, String> {
    core::write_text_file(path, text)
}

#[tauri::command]
pub async fn stat_path(path: String) -> Option<core::StatResult> {
    core::stat_path(path)
}

#[tauri::command]
pub async fn write_binary_file(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected raw bytes.".into());
    };
    let path = request
        .headers()
        .get("path")
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .ok_or("Missing path.")?;
    core::write_binary(std::path::Path::new(&path), bytes).map_err(|e| core::describe(&e))
}

#[tauri::command]
pub async fn copy_file(from: String, to: String) -> Result<(), String> {
    core::copy_file(from, to)
}

#[tauri::command]
pub fn pending_assets_dir(app: AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let context = calmmd_core::AppContext::headless(data_dir);
    core::pending_assets_dir(&context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_encoded_paths() {
        assert_eq!(percent_decode("C%3A%5CUsers%5Cme%5Cmy%20pic.png"), r"C:\Users\me\my pic.png");
        assert_eq!(percent_decode("caf%C3%A9.png"), "café.png");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
