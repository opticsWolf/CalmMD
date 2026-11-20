mod platform;
mod services;
mod state;
mod ui;

use platform::{NativePlatform, Platform};
use services::Services;
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

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);

        let mut services = Arc::new(Services::new(data_dir()));
        // The receiver is moved out once, when the GPUI event bridge is installed.
        let rx = Arc::get_mut(&mut services)
            .expect("services are not shared yet")
            .events
            .take()
            .expect("core event receiver");
        let events = CoreEvents::new(rx, cx);
        let platform: Arc<dyn Platform> = Arc::new(NativePlatform);

        ui::init(cx, services, events, platform);
    });
}
