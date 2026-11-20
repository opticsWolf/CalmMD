//! Tauri command wrappers for UI-independent `calmmd-core` services.

use crate::context::AppState;
use calmmd_core::core as cc;
use calmmd_core::mcp::{McpManager, McpStatus};
use serde_json::Value;
use tauri::{State, Window};
use std::sync::Arc;

// Markdown ------------------------------------------------------------------

#[tauri::command]
pub fn markdown_parse(text: String) -> Result<cc::dto::MarkdownParseResult, String> {
    cc::markdown_parse(text)
}

#[tauri::command]
pub fn markdown_render(
    text: String,
    options: Option<cc::dto::MarkdownRenderOptions>,
) -> Result<cc::dto::MarkdownRenderResult, String> {
    cc::markdown_render(text, options)
}

#[tauri::command]
pub fn markdown_highlight_code(
    language: String,
    code: String,
    theme: Option<String>,
) -> cc::dto::HighlightCodeResult {
    cc::markdown_highlight_code(language, code, theme)
}

// Workspace -----------------------------------------------------------------

#[tauri::command]
pub fn core_workspace_sync(
    window: Window,
    manager: State<'_, Arc<cc::workspace::WorkspaceRegistry>>,
    snapshot: cc::dto::WorkspaceSnapshotDto,
) -> Result<(), String> {
    manager.sync(window.label(), snapshot)
}

#[tauri::command]
pub fn core_workspace_upsert_document(
    window: Window,
    manager: State<'_, Arc<cc::workspace::WorkspaceRegistry>>,
    document: cc::dto::WorkspaceDocumentDto,
    active_document: Option<String>,
) -> Result<(), String> {
    manager.upsert_document(window.label(), document, active_document)
}

// Buffers -------------------------------------------------------------------

#[tauri::command]
pub fn core_buffer_draft_write(
    state: State<'_, AppState>,
    id: String,
    record: cc::buffers::DraftRecord,
) -> Result<(), String> {
    cc::buffers::app_store(&state.context).write_draft(&id, &record)
}

#[tauri::command]
pub fn core_buffer_draft_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    cc::buffers::app_store(&state.context).delete_draft(&id)
}

#[tauri::command]
pub fn core_buffer_drafts(state: State<'_, AppState>) -> Result<Vec<cc::buffers::DraftEntry>, String> {
    cc::buffers::app_store(&state.context).drafts()
}

#[tauri::command]
pub fn core_buffer_persist(
    state: State<'_, AppState>,
    request: cc::buffers::PersistRequest,
) -> Result<cc::buffers::PersistResult, String> {
    cc::buffers::app_store(&state.context).persist(request)
}

#[tauri::command]
pub fn core_buffer_load(state: State<'_, AppState>, path: String) -> Result<cc::buffers::DecodedFile, String> {
    cc::buffers::app_store(&state.context).load(&path)
}

// Autosave ------------------------------------------------------------------

#[tauri::command]
pub fn core_autosave_touch(
    service: State<'_, cc::autosave::AppAutosave>,
    request: cc::autosave::AutosaveRequest,
) -> cc::autosave::AutosaveState {
    service.touch(request)
}

#[tauri::command]
pub fn core_autosave_flush(
    service: State<'_, cc::autosave::AppAutosave>,
    id: String,
) -> cc::autosave::AutosaveState {
    service.flush(&id)
}

#[tauri::command]
pub fn core_autosave_status(
    service: State<'_, cc::autosave::AppAutosave>,
    id: String,
) -> Option<cc::autosave::AutosaveState> {
    service.status(&id)
}

#[tauri::command]
pub fn core_autosave_resolve(
    service: State<'_, cc::autosave::AppAutosave>,
    id: String,
    choice: cc::autosave::ResolveChoice,
) -> Result<cc::autosave::ResolveResult, String> {
    service.resolve(&id, choice)
}

#[tauri::command]
pub fn core_autosave_dispose(service: State<'_, cc::autosave::AppAutosave>, id: String) {
    service.dispose(&id);
}

// History -------------------------------------------------------------------

#[tauri::command]
pub fn core_history_list(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<cc::history::Version>, String> {
    cc::history::app_history(&state.context).list(&path)
}

#[tauri::command]
pub fn core_history_read(
    state: State<'_, AppState>,
    path: String,
    id: String,
) -> Result<Option<String>, String> {
    cc::history::app_history(&state.context).read(&path, &id)
}

#[tauri::command]
pub fn core_history_save(
    state: State<'_, AppState>,
    path: String,
    text: String,
    name: Option<String>,
) -> Result<Option<cc::history::Version>, String> {
    cc::history::app_history(&state.context).save(&path, &text, name.as_deref())
}

#[tauri::command]
pub fn core_history_remove(
    state: State<'_, AppState>,
    path: String,
    id: String,
) -> Result<(), String> {
    cc::history::app_history(&state.context).remove(&path, &id)
}

#[tauri::command]
pub fn core_history_rename(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), String> {
    cc::history::app_history(&state.context).rename(&from, &to)
}

#[tauri::command]
pub fn core_history_clear_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    cc::history::app_history(&state.context).clear_file(&path)
}

#[tauri::command]
pub fn core_history_clear_all(state: State<'_, AppState>) -> Result<(), String> {
    cc::history::app_history(&state.context).clear_all()
}

#[tauri::command]
pub fn core_history_usage(state: State<'_, AppState>) -> Result<cc::history::HistoryUsage, String> {
    cc::history::app_history(&state.context).usage()
}

// Sessions and Spaces -------------------------------------------------------

#[tauri::command]
pub fn core_session_get(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<cc::session::SessionDto>, String> {
    cc::session::app_store(&state.context).session(&key)
}

#[tauri::command]
pub fn core_session_set(
    state: State<'_, AppState>,
    key: String,
    session: cc::session::SessionDto,
) -> Result<(), String> {
    cc::session::app_store(&state.context).set_session(&key, &session)
}

#[tauri::command]
pub fn core_space_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<cc::session::SpaceDto>, String> {
    cc::session::app_store(&state.context).space(&id)
}

#[tauri::command]
pub fn core_space_save(state: State<'_, AppState>, space: cc::session::SpaceDto) -> Result<(), String> {
    cc::session::app_store(&state.context).save_space(&space)
}

#[tauri::command]
pub fn core_space_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    cc::session::app_store(&state.context).remove_space(&id)
}

#[tauri::command]
pub fn core_spaces(state: State<'_, AppState>) -> Result<Vec<cc::session::SpaceDto>, String> {
    cc::session::app_store(&state.context).spaces()
}

// Export --------------------------------------------------------------------

#[tauri::command]
pub fn core_export_assemble(payload: Value) -> Result<Value, String> {
    cc::export::assemble::assemble_from_value(payload)
}

#[tauri::command]
pub fn takumi_pdf(
    html: String,
    out: String,
    options: cc::export::takumi::TakumiPdfOptions,
) -> Result<(), String> {
    cc::export::takumi::render_pdf_file(&html, &out, &options)
}

// Spellchecking -------------------------------------------------------------

#[tauri::command]
pub fn core_spell_check(
    state: State<'_, AppState>,
    word: String,
    lang: Option<String>,
) -> Result<Option<cc::spell::SpellCheckDto>, String> {
    cc::spell::check(&state.context, &word, lang.as_deref())
}

#[tauri::command]
pub fn core_spell_check_text(
    state: State<'_, AppState>,
    text: String,
    lang: Option<String>,
) -> Result<Option<Vec<(u32, u32)>>, String> {
    cc::spell::check_text(&state.context, &text, lang.as_deref())
}

#[tauri::command]
pub fn core_spell_learn(
    state: State<'_, AppState>,
    word: String,
    lang: Option<String>,
) -> Result<bool, String> {
    cc::spell::learn(&state.context, &word, lang.as_deref())
}

#[tauri::command]
pub fn core_spell_dictionaries(
    state: State<'_, AppState>,
) -> Result<Vec<cc::spell::DictionaryInfo>, String> {
    cc::spell::dictionaries(&state.context)
}

#[tauri::command]
pub async fn core_spell_dictionary_install(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    cc::spell::install_dictionary(&state.context, &id).await
}

#[tauri::command]
pub fn core_spell_dictionary_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    cc::spell::remove_dictionary(&state.context, &id)
}

// MCP -----------------------------------------------------------------------

#[tauri::command]
pub async fn mcp_status(manager: State<'_, Arc<McpManager>>) -> Result<McpStatus, String> {
    Ok(manager.status().await)
}

#[tauri::command]
pub async fn mcp_start(manager: State<'_, Arc<McpManager>>) -> Result<McpStatus, String> {
    manager.start().await
}

#[tauri::command]
pub async fn mcp_stop(manager: State<'_, Arc<McpManager>>) -> Result<McpStatus, String> {
    Ok(manager.stop().await)
}

#[tauri::command]
pub async fn mcp_respond(manager: State<'_, Arc<McpManager>>, response: Value) -> Result<(), String> {
    manager.respond(response).await
}
