//! Tauri command wrapper for the core file search.

use calmmd_core::search::{search, SearchHit};

#[tauri::command]
pub async fn search_files(root: String, query: String, case_sensitive: bool, whole_word: bool) -> Vec<SearchHit> {
    tauri::async_runtime::spawn_blocking(move || {
        search(std::path::Path::new(&root), &query, case_sensitive, whole_word)
    })
    .await
    .unwrap_or_default()
}
