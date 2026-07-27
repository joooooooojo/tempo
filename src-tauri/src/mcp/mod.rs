mod auth;
mod server;

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::db::{load_settings, AppState, DEFAULT_MCP_PORT};

pub use server::TempoMcpServer;

struct McpRuntimeState {
    cancel: Option<CancellationToken>,
}

#[derive(Clone)]
pub struct McpController {
    inner: Arc<Mutex<McpRuntimeState>>,
    tools_changed: broadcast::Sender<()>,
}

impl McpController {
    pub fn new() -> Self {
        let (tools_changed, _) = broadcast::channel(16);
        Self {
            inner: Arc::new(Mutex::new(McpRuntimeState { cancel: None })),
            tools_changed,
        }
    }

    pub fn subscribe_tools_changed(&self) -> broadcast::Receiver<()> {
        self.tools_changed.subscribe()
    }

    pub fn notify_tools_changed(&self) {
        let _ = self.tools_changed.send(());
    }

    pub fn stop(&self) {
        let mut guard = self.inner.lock();
        if let Some(token) = guard.cancel.take() {
            token.cancel();
        }
    }

    pub fn restart(&self, app: &AppHandle) {
        self.stop();
        self.start(app);
    }

    pub fn start(&self, app: &AppHandle) {
        let Some(state) = app.try_state::<AppState>() else {
            tracing::warn!("MCP server skipped: AppState not ready");
            return;
        };

        let (enabled, port, token) = {
            let conn = state.db.lock();
            let settings = load_settings(&conn);
            (settings.mcp_enabled, settings.mcp_port, settings.mcp_token)
        };

        if !enabled {
            tracing::info!("MCP server disabled in settings");
            return;
        }

        if token.trim().is_empty() {
            tracing::warn!("MCP server skipped: empty token");
            return;
        }

        let port = if port == 0 { DEFAULT_MCP_PORT } else { port };
        let cancel = CancellationToken::new();
        {
            let mut guard = self.inner.lock();
            if let Some(previous) = guard.cancel.take() {
                previous.cancel();
            }
            guard.cancel = Some(cancel.clone());
        }

        let app = app.clone();
        let expected_token = Arc::new(token);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = run_mcp_http(app, port, expected_token, cancel).await {
                tracing::error!(error = %error, port, "MCP server exited with error");
            }
        });
    }
}

pub fn notify_plugin_tools_changed(app: &AppHandle) {
    if let Some(controller) = app.try_state::<McpController>() {
        controller.notify_tools_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_broadcasts_tool_changes() {
        let controller = McpController::new();
        let mut receiver = controller.subscribe_tools_changed();
        controller.notify_tools_changed();
        assert!(receiver.try_recv().is_ok());
    }
}

async fn run_mcp_http(
    app: AppHandle,
    port: u16,
    expected_token: Arc<String>,
    cancel: CancellationToken,
) -> Result<(), String> {
    use axum::{routing::get, Router};
    use rmcp::transport::{
        streamable_http_server::{
            session::local::LocalSessionManager, tower::StreamableHttpService,
        },
        StreamableHttpServerConfig,
    };

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind MCP on {addr}: {e}"))?;

    let mcp_service = StreamableHttpService::new(
        {
            let app = app.clone();
            move || Ok(TempoMcpServer::new(app.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(cancel.child_token()),
    );

    let protected = Router::new().nest_service("/mcp", mcp_service).layer(
        axum::middleware::from_fn_with_state(expected_token, auth::require_bearer),
    );

    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(protected);

    tracing::info!(%addr, "MCP server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
            tracing::info!("MCP server shutting down");
        })
        .await
        .map_err(|e| format!("MCP serve error: {e}"))
}
