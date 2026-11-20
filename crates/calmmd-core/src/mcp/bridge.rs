use crate::app::WindowInfo;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Optional routing fields every tool accepts. Flattened into each tool's arguments.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TargetArgs {
    /// Window label to run the call in (default: main).
    #[serde(rename = "windowLabel")]
    pub window_label: Option<String>,
    /// Space id; resolved to the window that currently holds it.
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>,
}

/// Correlates MCP tool calls with responses from the running webview.
pub struct McpBridge {
    app: crate::app::AppContext,
}

impl McpBridge {
    pub fn new(app: crate::app::AppContext) -> Arc<Self> {
        Arc::new(Self {
            app,
        })
    }

    /// Resolves an optional target to a concrete window label.
    pub fn window_for(&self, target: &TargetArgs) -> Result<Option<String>, String> {
        if let Some(label) = &target.window_label {
            return Ok(Some(label.clone()));
        }
        if let Some(space) = &target.space_id {
            return Ok(self.app.space_holder(space));
        }
        Ok(None)
    }

    /// Window labels, titles and claimed Space ids for discovery.
    pub fn windows(&self) -> Result<Value, String> {
        let windows = self
            .app
            .windows()
            .into_iter()
            .map(|WindowInfo { label, title, space_id }| json!({
                "label": label,
                "title": title,
                "spaceId": space_id,
            }))
            .collect::<Vec<_>>();
        Ok(json!({ "windows": windows }))
    }

    pub async fn respond(&self, _response: Value) -> Result<(), String> {
        Err("The webview MCP response bridge is no longer used".into())
    }

}
