use crate::app::WindowInfo;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{oneshot, Mutex};

type Reply = Result<Value, String>;

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
    pending: Mutex<HashMap<String, oneshot::Sender<Reply>>>,
    next: AtomicU64,
}

impl McpBridge {
    pub fn new(app: crate::app::AppContext) -> Arc<Self> {
        Arc::new(Self {
            app,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(0),
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

    pub async fn call(&self, window: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        let id = format!(
            "mcp-{}-{}",
            std::process::id(),
            self.next.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        let label = window.unwrap_or("main");
        let payload = json!({
            "id": id,
            "windowLabel": label,
            "method": method,
            "params": params,
        });
        self.app.emit_to(label, "calmmd://mcp/request", payload);

        // A trash tool may wait for the user to confirm in the UI.
        let answered = tokio::time::timeout(Duration::from_secs(120), rx).await;
        self.pending.lock().await.remove(&id);
        match answered {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => Err("CalmMD closed the response channel".into()),
            Err(_) => Err("CalmMD did not respond in time".into()),
        }
    }

    pub async fn respond(&self, response: Value) -> Result<(), String> {
        let id = response
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Missing response id")?
            .to_string();
        let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let reply = if ok {
            Ok(response.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP request failed")
                    .to_string(),
            )
        };
        let tx = self
            .pending
            .lock()
            .await
            .remove(&id)
            .ok_or_else(|| format!("Unknown response id: {id}"))?;
        tx.send(reply).map_err(|_| "Response was no longer needed".into())
    }
}
