//! Tauri commands for the explorer; operations live in `calmmd-core`.

use calmmd_core::tree as core;
use std::path::Path;

#[tauri::command]
pub async fn list_dir(path: String, show_hidden: bool) -> Result<Vec<core::DirEntry>, String> {
    core::list(Path::new(&path), show_hidden).map_err(|e| calmmd_core::fsops::describe(&e))
}

#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    core::create_file(path)
}

#[tauri::command]
pub async fn create_dir(path: String) -> Result<(), String> {
    core::create_dir(path)
}

#[tauri::command]
pub async fn rename_path(from: String, to: String) -> Result<(), String> {
    core::rename(Path::new(&from), Path::new(&to))
}

#[tauri::command]
pub async fn trash_path(path: String) -> Result<(), String> {
    core::trash_path(path)
}
