use super::bridge::{McpBridge, TargetArgs};
use crate::core::{paths::basename, workspace::WorkspaceRegistry};
use rmcp::{
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        CallToolResult, ContentBlock, GetPromptResult, ListResourceTemplatesResult,
        ListResourcesResult, PaginatedRequestParams, PromptMessage, ReadResourceRequestParams,
        ReadResourceResponse,
        ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Role,
    },
    service::RequestContext,
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router,
    ErrorData as McpError, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArgs {
    /// Absolute path, or a path relative to the folder open in CalmMD.
    pub path: String,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListArgs {
    /// Only Markdown/text notes (default: true).
    pub markdown_only: Option<bool>,
    /// Only documents inside this folder (recursive).
    pub folder: Option<String>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    pub query: String,
    pub case_sensitive: Option<bool>,
    pub whole_word: Option<bool>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArgs {
    /// Folder for the new note (default: the open folder).
    pub folder: Option<String>,
    pub name: String,
    /// Optional initial Markdown content.
    pub text: Option<String>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameArgs {
    pub path: String,
    /// New file name, not a full path.
    pub name: String,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteArgs {
    pub path: String,
    /// Full replacement text (or use `text`).
    pub content: Option<String>,
    pub text: Option<String>,
    /// replace (default), append, or prepend.
    pub mode: Option<String>,
    /// Wait for the atomic save before returning (default: true).
    pub flush: Option<bool>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TrashArgs {
    pub path: String,
    /// Destructive actions require this to be true.
    pub confirm: Option<bool>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveArgs {
    pub path: String,
    /// Destination folder (inside the open folder).
    pub folder: String,
    /// Optional new file name.
    pub name: Option<String>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TriageArgs {
    /// Folder to inventory (default: the open folder).
    pub folder: Option<String>,
    /// Maximum number of notes to return (default: 200).
    pub limit: Option<u32>,
    /// Characters of live text per note (default: 600).
    pub excerpt_chars: Option<u32>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationMoveArgs {
    pub from: String,
    /// Target folder, relative to the open folder or absolute inside it.
    pub to_folder: String,
    /// Optional new file name when the current one is ambiguous.
    pub new_name: Option<String>,
    /// Why the note belongs in that folder.
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OrganizationPlanArgs {
    pub moves: Vec<OrganizationMoveArgs>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOrganizationArgs {
    /// Folder the plan applies to (default: the open folder).
    pub folder: Option<String>,
    pub plan: OrganizationPlanArgs,
    /// false validates and previews; true performs the moves.
    pub apply: Option<bool>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PdfArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margins: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_numbers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_before_h1: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportPrefsArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<PdfArgs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_images: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_author: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoverArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportArgs {
    /// Document to export.
    pub path: String,
    /// Destination path (inside the open folder).
    pub out: String,
    /// Export preferences; omitted fields keep CalmMD's defaults.
    pub prefs: Option<ExportPrefsArgs>,
    /// Cover page fields; providing this enables the cover. Omitted fields come from the note.
    pub cover: Option<CoverArgs>,
    #[serde(flatten)]
    pub target: TargetArgs,
}

#[derive(Clone)]
pub struct CalmMdTools {
    bridge: Arc<McpBridge>,
    workspace: Arc<WorkspaceRegistry>,
    #[allow(dead_code)] // kept alive by #[tool_router]
    tool_router: ToolRouter<Self>,
    #[allow(dead_code)] // kept alive by #[prompt_router]
    prompt_router: PromptRouter<Self>,
}

impl CalmMdTools {
    pub fn new(bridge: Arc<McpBridge>, workspace: Arc<WorkspaceRegistry>) -> Self {
        Self {
            bridge,
            workspace,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Resolves a tool target to the window that owns the Rust workspace.
    fn window(&self, target: &TargetArgs) -> Result<String, String> {
        Ok(WorkspaceRegistry::window_label(self.bridge.window_for(target)?.as_deref()))
    }
}

fn ok(value: Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn fail(message: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(message)]))
}

fn percent_decode(input: &str, plus_as_space: bool) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &input[i + 1..i + 3];
                let byte = u8::from_str_radix(hex, 16).map_err(|_| format!("Invalid % escape in URI: {hex}"))?;
                out.push(byte);
                i += 3;
            }
            b'%' => return Err("Truncated % escape in URI".into()),
            b'+' if plus_as_space => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "URI is not valid UTF-8".into())
}

/// Splits a `calmmd://...` URI into its path part and query parameters.
fn parse_resource_uri(uri: &str) -> Result<(String, HashMap<String, String>), String> {
    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    let mut params = HashMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(
            percent_decode(key, true)?,
            percent_decode(value, true)?,
        );
    }
    Ok((path.to_string(), params))
}

fn target_from_query(query: &HashMap<String, String>) -> TargetArgs {
    TargetArgs {
        window_label: query.get("windowLabel").cloned(),
        space_id: query.get("spaceId").cloned(),
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[tool_router]
impl CalmMdTools {
    #[tool(description = "Status of the running CalmMD app: open folder, open tabs, active document.")]
    async fn calmmd_status(&self) -> Result<CallToolResult, McpError> {
        let window = WorkspaceRegistry::window_label(None);
        let windows = self
            .bridge
            .windows()
            .ok()
            .and_then(|value| value.get("windows").cloned())
            .unwrap_or_else(|| json!([]));
        match self.workspace.status(&window, &window, windows) {
            Ok(value) => ok(value),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "List CalmMD windows with their labels and claimed Space ids.")]
    async fn calmmd_windows(&self) -> Result<CallToolResult, McpError> {
        match self.bridge.windows() {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "List documents in the folder currently open in CalmMD.")]
    async fn calmmd_list_documents(&self, Parameters(args): Parameters<ListArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.list_documents(
                &window,
                args.markdown_only.unwrap_or(true),
                args.folder.as_deref(),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Read a document from the live buffer (including unsaved edits) or from disk.")]
    async fn calmmd_read_document(&self, Parameters(args): Parameters<PathArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.read_document(&window, &args.path),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Search all Markdown files in the open folder.")]
    async fn calmmd_search(&self, Parameters(args): Parameters<SearchArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.search(
                &window,
                &args.query,
                args.case_sensitive.unwrap_or(false),
                args.whole_word.unwrap_or(false),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Headings of a document (line numbers are 0-based).")]
    async fn calmmd_outline(&self, Parameters(args): Parameters<PathArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.outline(&window, &args.path),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Create a new Markdown document in the open folder, optionally with initial text.")]
    async fn calmmd_create_document(&self, Parameters(args): Parameters<CreateArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.create_document(
                &window,
                args.folder.as_deref(),
                &args.name,
                args.text.as_deref(),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Create a folder inside the folder currently open in CalmMD.")]
    async fn calmmd_create_folder(&self, Parameters(args): Parameters<CreateArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.create_folder(&window, args.folder.as_deref(), &args.name),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Rename a document. CalmMD retargets open tabs, favorites and file history.")]
    async fn calmmd_rename_document(&self, Parameters(args): Parameters<RenameArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.rename_document(&window, &args.path, &args.name),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Move a document into another folder. CalmMD retargets tabs, favorites, file history and links.")]
    async fn calmmd_move_document(&self, Parameters(args): Parameters<MoveArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.move_document(
                &window,
                &args.path,
                &args.folder,
                args.name.as_deref(),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Write to a live document. The edit is undoable, autosaved, and snapshotted in file history.")]
    async fn calmmd_write_document(&self, Parameters(args): Parameters<WriteArgs>) -> Result<CallToolResult, McpError> {
        let content = args.content.or(args.text).unwrap_or_default();
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.write_document(
                &window,
                &args.path,
                &content,
                &args.mode.unwrap_or_else(|| "replace".into()),
                args.flush.unwrap_or(true),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Undo the last Rust-owned change to a document. The view is updated and the change can be redone.")]
    async fn calmmd_undo_document(&self, Parameters(args): Parameters<PathArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.undo_document(&window, &args.path),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Redo the last undone Rust-owned change to a document.")]
    async fn calmmd_redo_document(&self, Parameters(args): Parameters<PathArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.redo_document(&window, &args.path),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Move a document to the OS trash. Requires confirm: true.")]
    async fn calmmd_trash_document(&self, Parameters(args): Parameters<TrashArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.trash_document(&window, &args.path, args.confirm.unwrap_or(false)),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Inventory a folder's Markdown notes with live excerpts and headings for content-aware organisation.")]
    async fn calmmd_triage_folder(&self, Parameters(args): Parameters<TriageArgs>) -> Result<CallToolResult, McpError> {
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.triage_folder(
                &window,
                args.folder.as_deref(),
                args.limit.unwrap_or(200) as usize,
                args.excerpt_chars.unwrap_or(600) as usize,
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Validate or apply a folder organisation plan returned by calmmd_organize_folder.")]
    async fn calmmd_apply_organization(&self, Parameters(args): Parameters<ApplyOrganizationArgs>) -> Result<CallToolResult, McpError> {
        let plan = match serde_json::to_value(&args.plan) {
            Ok(value) => value,
            Err(e) => return fail(e.to_string()),
        };
        let result = match self.window(&args.target) {
            Ok(window) => self.workspace.apply_organization(
                &window,
                args.folder.as_deref(),
                &plan,
                args.apply.unwrap_or(false),
            ),
            Err(e) => Err(e),
        };
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    /// Resolves paths, live text and export options in Rust; only final rendering stays in the view.
    fn export_params(&self, args: &ExportArgs) -> Result<Value, String> {
        let window = self.window(&args.target)?;
        let path = self.workspace.resolve(&window, &args.path)?;
        let out = self.workspace.resolve(&window, &args.out)?;
        let doc = self.workspace.read_document(&window, &path)?;
        let text = doc
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "The document has no readable text".to_owned())?;
        let raw_prefs = serde_json::to_value(&args.prefs).map_err(|e| e.to_string())?;
        let prefs = crate::core::export::prefs::resolve_prefs(Some(&raw_prefs));
        let raw_cover = serde_json::to_value(&args.cover).map_err(|e| e.to_string())?;
        let cover = crate::core::export::prefs::resolve_cover(
            text,
            &basename(&path),
            Some(&raw_cover),
            &prefs,
        );
        Ok(json!({ "path": path, "out": out, "text": text, "prefs": prefs, "cover": cover }))
    }

    #[tool(description = "Export a document to PDF using CalmMD's renderer and print engine. Use prefs.cover or the cover argument for a cover page; omitted cover fields come from the note.")]
    async fn calmmd_export_pdf(&self, Parameters(args): Parameters<ExportArgs>) -> Result<CallToolResult, McpError> {
        let params = match self.export_params(&args) {
            Ok(params) => params,
            Err(e) => return fail(e),
        };
        let result = crate::core::export::body::export_pdf(
            params.get("path").and_then(Value::as_str).unwrap_or(""),
            params.get("out").and_then(Value::as_str).unwrap_or(""),
            params.get("text").and_then(Value::as_str).unwrap_or(""),
            params.get("prefs").unwrap_or(&Value::Null),
            params.get("cover").unwrap_or(&Value::Null),
        );
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Export a document to standalone HTML using CalmMD's renderer. Use prefs.cover or the cover argument for a cover page; omitted cover fields come from the note.")]
    async fn calmmd_export_html(&self, Parameters(args): Parameters<ExportArgs>) -> Result<CallToolResult, McpError> {
        let params = match self.export_params(&args) {
            Ok(params) => params,
            Err(e) => return fail(e),
        };
        let result = crate::core::export::body::export_html(
            params.get("path").and_then(Value::as_str).unwrap_or(""),
            params.get("out").and_then(Value::as_str).unwrap_or(""),
            params.get("text").and_then(Value::as_str).unwrap_or(""),
            params.get("prefs").unwrap_or(&Value::Null),
            params.get("cover").unwrap_or(&Value::Null),
        );
        match result {
            Ok(v) => ok(v),
            Err(e) => fail(e),
        }
    }
}

#[prompt_router]
impl CalmMdTools {
    #[prompt(name = "calmmd_summarize")]
    async fn summarize(
        &self,
        Parameters(PathArgs { path, target }): Parameters<PathArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let window = self.window(&target).map_err(|e| McpError::internal_error(e, None))?;
        let text = self
            .workspace
            .read_document(&window, &path)
            .map_err(|e| McpError::internal_error(e, None))?;
        let body = text.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Summarise this CalmMD note in Markdown. Keep it short; use headings only if the note is long.\n\n--- NOTE ---\n{body}\n--- END ---"
            ),
        )])
        .with_description("Summarise a document from the live CalmMD app"))
    }

    #[prompt(name = "calmmd_follow_up_guide")]
    async fn follow_up_guide(
        &self,
        Parameters(PathArgs { path, target }): Parameters<PathArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let window = self.window(&target).map_err(|e| McpError::internal_error(e, None))?;
        let text = self
            .workspace
            .read_document(&window, &path)
            .map_err(|e| McpError::internal_error(e, None))?;
        let body = text.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Write a practical follow-up guide for this CalmMD note. Use Markdown with a title, 3–6 numbered steps and a short checklist. Base every step on the note.\n\n--- NOTE ---\n{body}\n--- END ---"
            ),
        )])
        .with_description("Write a follow-up guide for a document"))
    }

    #[prompt(name = "calmmd_organize_folder")]
    async fn organize_folder(
        &self,
        Parameters(TriageArgs { folder, limit, excerpt_chars, target }): Parameters<TriageArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let window = self.window(&target).map_err(|e| McpError::internal_error(e, None))?;
        let inventory = self
            .workspace
            .triage_folder(
                &window,
                folder.as_deref(),
                limit.unwrap_or(200) as usize,
                excerpt_chars.unwrap_or(600) as usize,
            )
            .map_err(|e| McpError::internal_error(e, None))?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Organise these CalmMD notes into a handful of topic subfolders based on their actual content. \
                 Skim the excerpts and headings; group related notes, and rename only files whose name is ambiguous. \
                 Return ONLY JSON with this shape: {{\"moves\":[{{\"from\":\"...\",\"toFolder\":\"...\",\"newName\":\"...\",\"reason\":\"...\"}}]}}. \
                 Use paths from the inventory, keep subfolders inside the open folder, omit newName when the name is fine, \
                 and do not move notes that already sit in a sensible folder. Then call calmmd_apply_organization with apply:false first.\n\n--- INVENTORY ---\n{}\n--- END ---",
                pretty(&inventory),
            ),
        )])
        .with_description("Propose a content-aware folder organisation plan for a CalmMD folder"))
    }
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for CalmMdTools {
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("calmmd://status", "calmmd-status")
                .with_title("CalmMD status")
                .with_description("The main window's root, tabs and active document as JSON")
                .with_mime_type("application/json"),
            Resource::new("calmmd://windows", "calmmd-windows")
                .with_title("CalmMD windows")
                .with_description("Window labels and claimed Space ids as JSON")
                .with_mime_type("application/json"),
        ]))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new("calmmd://document/{path}", "calmmd-document")
                .with_title("CalmMD document")
                .with_description("Live text of a note; percent-encode the path and add ?windowLabel=… or ?spaceId=… to route")
                .with_mime_type("text/markdown"),
            ResourceTemplate::new("calmmd://folder/{path}", "calmmd-folder")
                .with_title("CalmMD folder")
                .with_description("Markdown files inside a folder; percent-encode the path")
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let (base, query) =
            parse_resource_uri(&request.uri).map_err(|e| McpError::invalid_params(e, None))?;
        let target = target_from_query(&query);
        let window = self
            .bridge
            .window_for(&target)
            .map_err(|e| McpError::internal_error(e, None))?;

        let contents = match base.as_str() {
            "calmmd://status" => {
                let label = WorkspaceRegistry::window_label(None);
                let windows = self
                    .bridge
                    .windows()
                    .ok()
                    .and_then(|value| value.get("windows").cloned())
                    .unwrap_or_else(|| json!([]));
                let value = self
                    .workspace
                    .status(&label, &label, windows)
                    .map_err(|e| McpError::internal_error(e, None))?;
                ResourceContents::text(pretty(&value), request.uri).with_mime_type("application/json")
            }
            "calmmd://windows" => {
                let value = self
                    .bridge
                    .windows()
                    .map_err(|e| McpError::internal_error(e, None))?;
                ResourceContents::text(pretty(&value), request.uri).with_mime_type("application/json")
            }
            path if path.starts_with("calmmd://document/") => {
                let raw = &path["calmmd://document/".len()..];
                let path = percent_decode(raw, false)
                    .map_err(|e| McpError::invalid_params(e, None))?;
                let label = WorkspaceRegistry::window_label(window.as_deref());
                let value = self
                    .workspace
                    .read_document(&label, &path)
                    .map_err(|e| McpError::internal_error(e, None))?;
                let text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                ResourceContents::text(text, request.uri).with_mime_type("text/markdown")
            }
            path if path.starts_with("calmmd://folder/") => {
                let raw = &path["calmmd://folder/".len()..];
                let path = percent_decode(raw, false)
                    .map_err(|e| McpError::invalid_params(e, None))?;
                let label = WorkspaceRegistry::window_label(window.as_deref());
                let value = self
                    .workspace
                    .list_documents(&label, true, Some(&path))
                    .map_err(|e| McpError::internal_error(e, None))?;
                ResourceContents::text(pretty(&value), request.uri).with_mime_type("application/json")
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!("Unknown CalmMD resource: {}", request.uri),
                    None,
                ));
            }
        };

        Ok(ReadResourceResult::new(vec![contents]).into())
    }

    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::from_build_env())
        .with_protocol_version(rmcp::model::ProtocolVersion::V_2026_07_28)
        .with_instructions(
            "Tools operate on the live CalmMD app. Use prompts calmmd_summarize / \
             calmmd_follow_up_guide for generated text, then calmmd_write_document to save it. \
             Use calmmd_organize_folder with calmmd_apply_organization for content-aware folder triage. \
             Read-only URIs: calmmd://status, calmmd://windows, calmmd://document/{path}, \
             calmmd://folder/{path}."
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_resource_paths_and_query_targets() {
        let (path, query) = parse_resource_uri(
            "calmmd://document/C%3A%2Fnotes%2Fa%20b.md?spaceId=writing+desk&windowLabel=main",
        )
        .unwrap();
        let raw = &path["calmmd://document/".len()..];
        assert_eq!(percent_decode(raw, false).unwrap(), "C:/notes/a b.md");
        assert_eq!(query.get("spaceId").map(String::as_str), Some("writing desk"));
        let target = target_from_query(&query);
        assert_eq!(target.space_id.as_deref(), Some("writing desk"));
        assert_eq!(target.window_label.as_deref(), Some("main"));
    }

    #[test]
    fn rejects_invalid_percent_escapes() {
        assert!(percent_decode("a%ZZ.md", false).is_err());
        assert!(parse_resource_uri("calmmd://document/a.md?x=%ZZ").is_err());
    }
}
