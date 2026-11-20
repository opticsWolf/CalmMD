mod editor;
mod platform;
mod preview;
mod services;
mod single_instance;
mod state;
mod theme;
mod ui;
mod menu_dialogs;

use platform::{NativePlatform, Platform};
use calmmd_core::core::dto::WorkspaceSnapshotDto;
use gpui_kit::ClipboardItem;
use services::{CoreEvent, Services};
use single_instance::{launch_paths, queue_launch_paths, take_launch_paths, InstanceLock};
use state::CoreEvents;
use std::{path::PathBuf, sync::Arc};

fn data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CALMMD_DATA_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(root) = std::env::var("APPDATA") {
            return PathBuf::from(root).join("CalmMD");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("CalmMD");
        }
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Ok(root) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(root).join("CalmMD");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local").join("share").join("CalmMD");
        }
    }
    std::env::temp_dir().join("CalmMD")
}

fn value_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn open_paths_event(paths: Vec<String>) -> CoreEvent {
    CoreEvent {
        target: None,
        name: "calmmd://open-paths".into(),
        payload: serde_json::Value::Array(paths.into_iter().map(serde_json::Value::String).collect()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data_dir = data_dir();
    let initial_paths = launch_paths(args.iter().cloned());
    let mut root_override = None;
    let mut idx = 1;
    while idx < args.len() {
        if args[idx] == "--root" {
            root_override = args.get(idx + 1).cloned();
            idx += 2;
        } else {
            idx += 1;
        }
    }
    let dark_theme = value_arg(&args, "--theme")
        .map(|value| value.eq_ignore_ascii_case("dark"))
        .or_else(|| std::env::var_os("CALMMD_THEME").map(|value| value == "dark"))
        .unwrap_or(false);
    let mcp_enabled = args.iter().any(|arg| arg == "--mcp")
        || std::env::var_os("CALMMD_MCP").is_some_and(|value| value != "0");
    let Some(_lock) = InstanceLock::acquire(&data_dir) else {
        queue_launch_paths(&data_dir, initial_paths);
        eprintln!("CalmMD is already running; launch paths were queued for it.");
        return;
    };

    let mut services = Arc::new(Services::new(data_dir.clone()));
    if let Some(root) = root_override {
        let _ = services.workspace.sync(
            "main",
            WorkspaceSnapshotDto {
                root: Some(root),
                documents: Vec::new(),
                active_document: None,
            },
        );
    }

    // rmcp and axum need a Tokio reactor. GPUI's executor drives the views; this runtime drives
    // the loopback MCP listener independently.
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime for MCP");
    let mcp_status = if mcp_enabled {
        match runtime.block_on(services.start_mcp()) {
            Ok(status) => {
                println!("CALMMD_MCP_URL={}", status.url.as_deref().unwrap_or(""));
                println!("CALMMD_MCP_TOKEN={}", status.token.as_deref().unwrap_or(""));
                Some(status)
            }
            Err(error) => {
                eprintln!("Could not start MCP: {error}");
                None
            }
        }
    } else {
        None
    };

    let launch_tx = services.event_sender.clone();
    let open_urls_tx = launch_tx.clone();
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.on_open_urls(move |urls| {
        let paths: Vec<String> = urls
            .into_iter()
            .map(|url| {
                if let Some(path) = url.strip_prefix("file://") {
                    path.to_owned()
                } else {
                    url
                }
            })
            .collect();
        let _ = open_urls_tx.send(open_paths_event(paths));
    });

    app.run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        if dark_theme {
            theme::set(cx, true);
        }

        // The receiver is moved out once, when the GPUI event bridge is installed.
        let rx = Arc::get_mut(&mut services)
            .expect("services are not shared yet")
            .events
            .take()
            .expect("core event receiver");
        let events = CoreEvents::new(rx, cx);
        let platform: Arc<dyn Platform> = Arc::new(NativePlatform);

        ui::init(cx, services, events, platform);

        if let Some(status) = mcp_status {
            if let (Some(url), Some(token)) = (status.url, status.token) {
                cx.write_to_clipboard(ClipboardItem::new_string(format!(
                    "{url} · token: {token}"
                )));
            }
        }

        let queued = take_launch_paths(&data_dir);
        for path in initial_paths.into_iter().chain(queued) {
            let _ = launch_tx.send(open_paths_event(vec![path]));
        }

        // A second process writes launch paths to the queue; the owner polls it so files opened
        // from the shell or file manager reach the already-running window.
        let poll_tx = launch_tx.clone();
        let poll_dir = data_dir.clone();
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
            for path in take_launch_paths(&poll_dir) {
                let _ = poll_tx.send(open_paths_event(vec![path]));
            }
        })
        .detach();
    });
}
