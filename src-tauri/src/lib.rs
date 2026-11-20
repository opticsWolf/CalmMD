mod cdp;
mod commands;
mod context;
mod export;
mod fsops;
mod kv;
mod search;
mod spell;
mod tree;
mod windows;

use calmmd_core::{
    core::{autosave, workspace::WorkspaceRegistry},
    mcp::McpManager,
    windows::SpaceClaims,
};
use context::AppState;
use std::{path::Path, sync::Arc};
use tauri::{Emitter, Manager};

fn file_args<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .filter(|a| !a.starts_with('-'))
        .filter(|a| Path::new(a).is_file())
        .map(|a| {
            std::path::absolute(&a)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(a)
        })
        .collect()
}

/// Files passed on the command line (also how "Open with" works on Windows and Linux).
#[tauri::command]
fn launch_paths() -> Vec<String> {
    file_args(std::env::args().skip(1))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let claims = Arc::new(SpaceClaims::default());
            let context = context::app_context(app.handle(), claims.clone());
            app.manage(AppState {
                context: context.clone(),
                claims: claims.clone(),
            });

            let workspace = WorkspaceRegistry::new(context.clone());
            app.manage(workspace.clone());
            let manager = McpManager::new(context.clone(), workspace);
            app.manage(manager.clone());
            app.manage(autosave::create_app_autosave(&context));

            // `--mcp` starts the loopback server at launch for clients that expect it.
            if std::env::args().any(|a| a == "--mcp") {
                tauri::async_runtime::spawn(async move {
                    let _ = manager.start().await;
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let paths = file_args(args.into_iter().skip(1));
            let target = app
                .get_webview_window("main")
                .or_else(|| app.webview_windows().into_values().next());
            if let Some(w) = target {
                let _ = w.unminimize();
                let _ = w.set_focus();
                if !paths.is_empty() {
                    let _ = app.emit_to(w.label(), "calmmd://open-paths", paths);
                }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<AppState>().claims.release(window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            fsops::read_text_file,
            fsops::write_text_file,
            fsops::stat_path,
            fsops::write_binary_file,
            fsops::copy_file,
            fsops::pending_assets_dir,
            tree::list_dir,
            search::search_files,
            tree::create_file,
            tree::create_dir,
            tree::rename_path,
            tree::trash_path,
            kv::kv_get,
            kv::kv_set,
            kv::kv_delete,
            kv::kv_list,
            windows::claim_space,
            windows::space_window,
            windows::open_window,
            windows::focus_window,
            windows::window_labels,
            export::find_browser,
            export::print_pdf,
            export::read_image_data_url,
            export::find_pandoc,
            export::pandoc_convert,
            export::themes_dir,
            spell::spell_check,
            spell::spell_check_text,
            spell::spell_learn,
            commands::mcp_status,
            commands::mcp_start,
            commands::mcp_stop,
            commands::mcp_new_token,
            commands::mcp_respond,
            commands::markdown_parse,
            commands::markdown_render,
            commands::markdown_highlight_code,
            commands::core_workspace_sync,
            commands::core_workspace_upsert_document,
            commands::core_buffer_draft_write,
            commands::core_buffer_draft_delete,
            commands::core_buffer_drafts,
            commands::core_buffer_persist,
            commands::core_buffer_load,
            commands::core_autosave_touch,
            commands::core_autosave_flush,
            commands::core_autosave_status,
            commands::core_autosave_resolve,
            commands::core_history_list,
            commands::core_history_read,
            commands::core_history_save,
            commands::core_history_remove,
            commands::core_history_rename,
            commands::core_history_clear_file,
            commands::core_history_clear_all,
            commands::core_history_usage,
            commands::core_autosave_dispose,
            commands::core_session_get,
            commands::core_session_set,
            commands::core_space_get,
            commands::core_space_save,
            commands::core_space_remove,
            commands::core_spaces,
            commands::core_export_assemble,
            commands::takumi_pdf,
            commands::core_spell_check,
            commands::core_spell_check_text,
            commands::core_spell_learn,
            commands::core_spell_dictionaries,
            commands::core_spell_dictionary_install,
            commands::core_spell_dictionary_remove,
            launch_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CalmMD");
}
