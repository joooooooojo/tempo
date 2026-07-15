mod auth;
mod server;

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::db::{load_settings, AppState, DEFAULT_MCP_PORT};

pub use server::TempoMcpServer;

struct McpRuntimeState {
    cancel: Option<CancellationToken>,
}

#[derive(Clone)]
pub struct McpController {
    inner: Arc<Mutex<McpRuntimeState>>,
}

impl McpController {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(McpRuntimeState { cancel: None })),
        }
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

async fn run_mcp_http(
    app: AppHandle,
    port: u16,
    expected_token: Arc<String>,
    cancel: CancellationToken,
) -> Result<(), String> {
    use axum::{Router, routing::get};
    use rmcp::transport::{
        StreamableHttpServerConfig,
        streamable_http_server::{
            session::local::LocalSessionManager, tower::StreamableHttpService,
        },
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
