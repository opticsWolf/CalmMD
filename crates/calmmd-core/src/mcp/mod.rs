//! Local MCP server. It never touches files by itself: every tool call is routed to the
//! running webview, where the document manager and explorer own the live state.

mod bridge;
mod tools;

use bridge::McpBridge;
use crate::core::workspace::WorkspaceRegistry;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::Serialize;
use std::sync::Arc;
use crate::app::AppContext;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tools::CalmMdTools;
use uuid::Uuid;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub url: Option<String>,
    pub token: Option<String>,
    pub port: Option<u16>,
    pub window_label: String,
}

struct Running {
    stop: watch::Sender<bool>,
    status: McpStatus,
}

pub struct McpManager {
    bridge: Arc<McpBridge>,
    workspace: Arc<WorkspaceRegistry>,
    running: Mutex<Option<Running>>,
}

impl McpManager {
    pub fn new(app: AppContext, workspace: Arc<WorkspaceRegistry>) -> Arc<Self> {
        Arc::new(Self {
            bridge: McpBridge::new(app),
            workspace,
            running: Mutex::new(None),
        })
    }

    pub async fn status(&self) -> McpStatus {
        match self.running.lock().await.as_ref() {
            Some(r) => r.status.clone(),
            None => McpStatus {
                enabled: false,
                url: None,
                token: None,
                port: None,
                window_label: "main".into(),
            },
        }
    }

    pub async fn start(&self) -> Result<McpStatus, String> {
        if let Some(r) = self.running.lock().await.as_ref() {
            return Ok(r.status.clone());
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = Uuid::new_v4().to_string();
        let (stop, mut stop_rx) = watch::channel(false);

        let bridge = self.bridge.clone();
        let workspace = self.workspace.clone();
        let service = StreamableHttpService::new(
            move || Ok(CalmMdTools::new(bridge.clone(), workspace.clone())),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );
        let router = axum::Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn_with_state(
                AuthState {
                    token: token.clone(),
                },
                authenticate,
            ));

        tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = stop_rx.changed().await;
                })
                .await;
        });

        let status = McpStatus {
            enabled: true,
            url: Some(format!("http://127.0.0.1:{port}/mcp")),
            token: Some(token),
            port: Some(port),
            window_label: "main".into(),
        };
        *self.running.lock().await = Some(Running {
            stop,
            status: status.clone(),
        });
        Ok(status)
    }

    pub async fn respond(&self, response: serde_json::Value) -> Result<(), String> {
        self.bridge.respond(response).await
    }

    pub async fn stop(&self) -> McpStatus {
        if let Some(r) = self.running.lock().await.take() {
            let _ = r.stop.send(true);
        }
        McpStatus {
            enabled: false,
            url: None,
            token: None,
            port: None,
            window_label: "main".into(),
        }
    }
}

#[derive(Clone)]
struct AuthState {
    token: String,
}

/// MCP's Streamable HTTP security guidance: local binding, authentication and Origin checks.
async fn authenticate(
    axum::extract::State(state): axum::extract::State<AuthState>,
    req: axum::extract::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    use axum::http::header;

    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {}", state.token) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    if let Some(origin) = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
    {
        if !is_local_origin(origin) {
            return Err(axum::http::StatusCode::FORBIDDEN);
        }
    }
    Ok(next.run(req).await)
}

fn is_local_origin(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = rest
        .split([':', '/'])
        .next()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_origins_pass() {
        assert!(is_local_origin("null"));
        assert!(is_local_origin("http://127.0.0.1:1420"));
        assert!(is_local_origin("http://localhost"));
        assert!(!is_local_origin("http://localhost.evil.example"));
        assert!(!is_local_origin("https://example.com"));
    }
}

