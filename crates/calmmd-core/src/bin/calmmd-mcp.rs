//! Headless CalmMD MCP host. It starts the same core services as the shells, with no UI process.

use calmmd_core::{
    app::WindowRegistry,
    core::{
        dto::WorkspaceSnapshotDto,
        session::app_store,
        workspace::WorkspaceRegistry,
    },
    mcp::McpManager,
    AppContext, WindowInfo,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Default)]
struct MainWindows;

impl WindowRegistry for MainWindows {
    fn windows(&self) -> Vec<WindowInfo> {
        vec![WindowInfo {
            label: "main".into(),
            title: "CalmMD headless".into(),
            space_id: None,
        }]
    }
}

fn value_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn data_dir(args: &[String]) -> PathBuf {
    value_arg(args, "--data-dir")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CALMMD_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir)
        .join("calmmd-mcp")
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let data_dir = data_dir(&args);
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    let app = AppContext::new(
        data_dir,
        Arc::new(calmmd_core::app::NullEvents),
        Arc::new(MainWindows),
    );
    let workspace = WorkspaceRegistry::new(app.clone());
    let session = app_store(&app);
    let root = value_arg(&args, "--root")
        .or_else(|| session.session("main").ok().flatten().and_then(|s| s.root))
        .ok_or_else(|| "Pass --root <folder> or open a folder in CalmMD first".to_owned())?;
    if !PathBuf::from(&root).is_dir() {
        return Err(format!("Folder does not exist: {root}"));
    }
    workspace.sync(
        "main",
        WorkspaceSnapshotDto {
            root: Some(root),
            documents: Vec::new(),
            active_document: None,
        },
    )?;

    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let manager = McpManager::new(app, workspace);
        let status = manager.start().await?;
        println!("CALMMD_MCP_URL={}", status.url.as_deref().unwrap_or(""));
        println!("CALMMD_MCP_TOKEN={}", status.token.as_deref().unwrap_or(""));
        std::future::pending::<()>().await;
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    })
}
