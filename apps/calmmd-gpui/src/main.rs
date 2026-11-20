mod editor;
mod platform;
mod preview;
mod services;
mod single_instance;
mod state;
mod theme;
mod ui;

use platform::{NativePlatform, Platform};
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

fn open_paths_event(paths: Vec<String>) -> CoreEvent {
    CoreEvent {
        target: None,
        name: "calmmd://open-paths".into(),
        payload: serde_json::Value::Array(paths.into_iter().map(serde_json::Value::String).collect()),
    }
}

fn main() {
    let data_dir = data_dir();
    let initial_paths = launch_paths(std::env::args());
    let Some(_lock) = InstanceLock::acquire(&data_dir) else {
        queue_launch_paths(&data_dir, initial_paths);
        eprintln!("CalmMD is already running; launch paths were queued for it.");
        return;
    };

    let mut services = Arc::new(Services::new(data_dir.clone()));
    let launch_tx = services.event_sender.clone();
    let open_urls_tx = launch_tx.clone();
    let app = gpui_kit::application();
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

        // The receiver is moved out once, when the GPUI event bridge is installed.
        let rx = Arc::get_mut(&mut services)
            .expect("services are not shared yet")
            .events
            .take()
            .expect("core event receiver");
        let events = CoreEvents::new(rx, cx);
        let platform: Arc<dyn Platform> = Arc::new(NativePlatform);

        ui::init(cx, services, events, platform);

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
