//! Host Bridge (design §7): RPC envelope, connection identity, method dispatch, rate limits,
//! timeouts. `dispatch` is the single entry point used by both the UI bridge
//! (`plugin_bridge_invoke`) and Runtime-initiated `host.*` calls relayed by the Supervisor.
//!
//! `ConnectionContext` is always constructed by the host from data it already trusts (the
//! Wry/iframe view instance registry, or the Supervisor's own child-process bookkeeping) —
//! callers can never self-report `plugin_id` inside the RPC payload.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::AppState;

use super::host::PluginHost;
use super::manifest::AppRect;
use super::storage;

/// RPC error codes (design §7). Kept as `&'static str` so callers can match without allocating.
pub mod codes {
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const PAYLOAD_TOO_LARGE: &str = "PAYLOAD_TOO_LARGE";
    pub const RESOURCE_EXHAUSTED: &str = "RESOURCE_EXHAUSTED";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const CANCELLED: &str = "CANCELLED";
    pub const ACTIVATION_FAILED: &str = "ACTIVATION_FAILED";
    pub const RUNTIME_UNAVAILABLE: &str = "RUNTIME_UNAVAILABLE";
    pub const COMMAND_FAILED: &str = "COMMAND_FAILED";
    pub const INTERNAL: &str = "INTERNAL";
}

/// Host Bridge API semver (design §7.2) — independent from the Tempo product version.
pub const HOST_API_VERSION: &str = "1.0.0";

/// Max single-message size (design §7): 1 MiB.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Max concurrent in-flight requests per plugin (design §7).
pub const MAX_CONCURRENT_PER_PLUGIN: usize = 32;
/// Default Host API timeout (design §7); interactive panel methods use a shorter one.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            data: None,
        }
    }

    /// `INTERNAL` must never leak Rust/Node internals (design §7): use this instead of
    /// forwarding a raw `Display` error to the plugin.
    pub fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        tracing::warn!(context = context, error = %error, "plugin bridge internal error");
        Self::new(codes::INTERNAL, format!("{context} failed"))
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Where a bridge call originated from. The host constructs this from data it already owns —
/// it is never trusted from the request payload itself (design §7, §5.3).
#[derive(Debug, Clone)]
pub enum ConnectionSource {
    Ui { view_instance_id: String },
    Runtime,
}

#[derive(Debug, Clone)]
pub struct ConnectionContext {
    pub plugin_id: String,
    pub source: ConnectionSource,
}

impl ConnectionContext {
    pub fn runtime(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            source: ConnectionSource::Runtime,
        }
    }

    pub fn ui(plugin_id: impl Into<String>, view_instance_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            source: ConnectionSource::Ui {
                view_instance_id: view_instance_id.into(),
            },
        }
    }

    pub fn view_instance_id(&self) -> Option<&str> {
        match &self.source {
            ConnectionSource::Ui { view_instance_id } => Some(view_instance_id.as_str()),
            ConnectionSource::Runtime => None,
        }
    }
}

/// RAII guard releasing the per-plugin in-flight slot acquired by [`acquire_slot`].
pub struct ConcurrencyGuard {
    host: Arc<PluginHost>,
    plugin_id: String,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        self.host.release_inflight_slot(&self.plugin_id);
    }
}

fn acquire_slot(host: &Arc<PluginHost>, plugin_id: &str) -> Result<ConcurrencyGuard, RpcError> {
    if !host.try_acquire_inflight_slot(plugin_id, MAX_CONCURRENT_PER_PLUGIN) {
        return Err(RpcError::new(
            codes::RESOURCE_EXHAUSTED,
            "too many concurrent requests for this plugin",
        ));
    }
    Ok(ConcurrencyGuard {
        host: host.clone(),
        plugin_id: plugin_id.to_string(),
    })
}

fn payload_too_large(params: &Value) -> bool {
    serde_json::to_vec(params)
        .map(|bytes| bytes.len() > MAX_MESSAGE_BYTES)
        .unwrap_or(false)
}

/// Shared Runtime Command boundary for Actions (not UI or MCP Tools).
pub async fn invoke_runtime_command(
    host: &Arc<PluginHost>,
    plugin_id: &str,
    command_id: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, RpcError> {
    if command_id.trim().is_empty() {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "missing runtime command id",
        ));
    }
    if payload_too_large(&params) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }
    let _guard = acquire_slot(host, plugin_id)?;
    host.supervisor
        .call(plugin_id, command_id, params, timeout)
        .await
}

/// Runtime-only MCP Tool boundary. Tool handlers use `tempo.mcpTools.register`.
pub async fn invoke_runtime_mcp_tool(
    host: &Arc<PluginHost>,
    plugin_id: &str,
    tool_name: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value, RpcError> {
    if tool_name.trim().is_empty() {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "missing MCP tool name",
        ));
    }
    if payload_too_large(&arguments) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }
    let _guard = acquire_slot(host, plugin_id)?;
    host.supervisor
        .call_mcp_tool(plugin_id, tool_name, arguments, timeout)
        .await
}

/// Private UI ↔ Runtime `ipc.invoke` (`ipc.invoke.*`). SCA envelope is opaque to the host.
pub async fn invoke_runtime_ipc(
    host: &Arc<PluginHost>,
    plugin_id: &str,
    channel: &str,
    args: Value,
    timeout: Duration,
) -> Result<Value, RpcError> {
    if channel.trim().is_empty() {
        return Err(RpcError::new(codes::INVALID_REQUEST, "missing ipc channel"));
    }
    if payload_too_large(&args) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }
    let _guard = acquire_slot(host, plugin_id)?;
    host.supervisor
        .call_ipc(plugin_id, channel, args, timeout)
        .await
}

/// Private UI ↔ Runtime fire-and-forget `ipc.send` (`ipc.send.*`). SCA envelope is opaque.
pub async fn send_runtime_ipc(
    host: &Arc<PluginHost>,
    plugin_id: &str,
    channel: &str,
    args: Value,
) -> Result<Value, RpcError> {
    if channel.trim().is_empty() {
        return Err(RpcError::new(codes::INVALID_REQUEST, "missing ipc channel"));
    }
    if payload_too_large(&args) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }
    let _guard = acquire_slot(host, plugin_id)?;
    host.supervisor.send_ipc(plugin_id, channel, args).await?;
    Ok(Value::Null)
}

/// UI / Runtime Host entry. UI may use `ipc.invoke.*` / `ipc.send.*` and Host methods —
/// never external Commands (those are for Actions via [`invoke_runtime_command`]).
pub async fn dispatch(
    app: &AppHandle,
    host: &Arc<PluginHost>,
    ctx: &ConnectionContext,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    if method.trim().is_empty() {
        return Err(RpcError::new(codes::INVALID_REQUEST, "method is required"));
    }

    if method.starts_with("runtime.") {
        return Err(RpcError::new(
            codes::FORBIDDEN,
            "UI cannot invoke Runtime Commands; use ipcRenderer.invoke for private UI↔Runtime IPC",
        ));
    }

    if let Some(channel) = method.strip_prefix("ipc.invoke.") {
        return invoke_runtime_ipc(host, &ctx.plugin_id, channel, params, DEFAULT_TIMEOUT).await;
    }

    if let Some(channel) = method.strip_prefix("ipc.send.") {
        return send_runtime_ipc(host, &ctx.plugin_id, channel, params).await;
    }

    // Legacy IPC method alias kept while the plugin architecture is still in development.
    if let Some(channel) = method.strip_prefix("rpc.") {
        return invoke_runtime_ipc(host, &ctx.plugin_id, channel, params, DEFAULT_TIMEOUT).await;
    }

    if payload_too_large(&params) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }

    let _guard = acquire_slot(host, &ctx.plugin_id)?;

    let timeout = if matches!(
        method,
        "mainPanel.setSize"
            | "mainPanel.back"
            | "mainPanel.hide"
            | "window.setRect"
            | "window.close"
    ) {
        INTERACTIVE_TIMEOUT
    } else {
        DEFAULT_TIMEOUT
    };

    tokio::time::timeout(
        timeout,
        dispatch_host_method(app, host, ctx, method, params),
    )
    .await
    .unwrap_or_else(|_| Err(RpcError::new(codes::TIMEOUT, "host API call timed out")))
}

async fn dispatch_host_method(
    app: &AppHandle,
    host: &Arc<PluginHost>,
    ctx: &ConnectionContext,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    match method {
        "mainPanel.hide" => {
            crate::auxiliary_windows::hide_main_panel(app)
                .map_err(|e| RpcError::internal("mainPanel.hide", e))?;
            Ok(Value::Null)
        }
        "mainPanel.back" => {
            let Some(view_instance_id) = ctx.view_instance_id() else {
                return Err(RpcError::new(codes::FORBIDDEN, "mainPanel.back is UI-only"));
            };
            app.emit(
                "plugin-host:main-panel-back",
                json!({ "viewInstanceId": view_instance_id }),
            )
            .map_err(|e| RpcError::internal("mainPanel.back", e))?;
            Ok(Value::Null)
        }
        "mainPanel.setSize" => {
            let Some(_view_instance_id) = ctx.view_instance_id() else {
                return Err(RpcError::new(
                    codes::FORBIDDEN,
                    "mainPanel.setSize is UI-only",
                ));
            };
            let height = params
                .get("height")
                .and_then(Value::as_f64)
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "height is required"))?;
            // Main panel chrome keeps a fixed search width; plugins may only change height.
            crate::auxiliary_windows::set_main_panel_size(app.clone(), None, height)
                .map_err(|e| RpcError::internal("mainPanel.setSize", e))?;
            Ok(Value::Null)
        }
        "window.setRect" => {
            let label = standalone_window_label(host, ctx)?;
            let rect = parse_window_rect(&params)?;
            super::windows::set_plugin_window_rect(app, &label, &rect)
                .map_err(|e| RpcError::internal("window.setRect", e))?;
            Ok(Value::Null)
        }
        "window.close" => {
            let label = standalone_window_label(host, ctx)?;
            super::windows::close_plugin_window_later(app, &label);
            Ok(Value::Null)
        }
        "app.open" => {
            let app_id = params
                .get("appId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "appId is required"))?;
            let open_params = params.get("params").cloned().unwrap_or(Value::Null);
            app.emit(
                "main-panel:open-app",
                json!({ "appId": app_id, "params": open_params }),
            )
            .map_err(|e| RpcError::internal("app.open", e))?;
            Ok(Value::Null)
        }
        "external.open" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "url is required"))?;
            if !(url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("mailto:"))
            {
                return Err(RpcError::new(
                    codes::FORBIDDEN,
                    "only http(s):// and mailto: URLs may be opened",
                ));
            }
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(url, None::<String>)
                .map_err(|e| RpcError::internal("external.open", e))?;
            Ok(Value::Null)
        }
        "notify.show" => {
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Tempo Plugin");
            let body = params.get("body").and_then(Value::as_str).unwrap_or("");
            // Permission / user-facing failures must keep their message — do not scrub via
            // `RpcError::internal`, or the main panel only shows a generic "操作没有完成".
            crate::notify::show_notification(app, title, body)
                .map_err(|message| RpcError::new(codes::FORBIDDEN, message))?;
            Ok(Value::Null)
        }
        "theme.get" => {
            let state = app
                .try_state::<AppState>()
                .ok_or_else(|| RpcError::internal("theme.get", "app state unavailable"))?;
            let theme = {
                let conn = state.db.lock();
                crate::db::get_setting(&conn, "theme", "system")
            };
            Ok(json!({ "theme": theme }))
        }
        "theme.onChange" => {
            let subscription_id = super::host::generate_id();
            host.register_subscription(
                &subscription_id,
                &ctx.plugin_id,
                "theme",
                ctx.view_instance_id(),
            );
            Ok(json!({ "subscriptionId": subscription_id }))
        }
        "session.push" => {
            // UI-only: `PluginAppHost` proactively pushes its latest serialized session state so
            // `plugin_ui_serialize_session` can answer without a round-trip (design §5.5).
            let Some(view_instance_id) = ctx.view_instance_id() else {
                return Err(RpcError::new(codes::FORBIDDEN, "session.push is UI-only"));
            };
            let payload = params.get("payload").cloned().unwrap_or(Value::Null);
            host.cache_session_payload(view_instance_id, payload);
            Ok(Value::Null)
        }
        "subscription.release" => {
            let subscription_id = params
                .get("subscriptionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RpcError::new(codes::INVALID_REQUEST, "subscriptionId is required")
                })?;
            host.release_subscription(subscription_id, &ctx.plugin_id);
            Ok(Value::Null)
        }
        "storage.plugin.get" => {
            let key = require_str(&params, "key")?;
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let namespace = host.plugin_storage_namespace(&ctx.plugin_id);
            let value = storage::get(&conn, &namespace, key)
                .map_err(|e| RpcError::internal("storage.plugin.get", e))?;
            Ok(json!({ "value": value }))
        }
        "storage.plugin.set" => {
            let key = require_str(&params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let namespace = host.plugin_storage_namespace(&ctx.plugin_id);
            storage::set(&conn, &namespace, key, &value)
                .map_err(|e| RpcError::new(codes::RESOURCE_EXHAUSTED, e))?;
            Ok(Value::Null)
        }
        "storage.plugin.delete" => {
            let key = require_str(&params, "key")?;
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let namespace = host.plugin_storage_namespace(&ctx.plugin_id);
            storage::delete(&conn, &namespace, key)
                .map_err(|e| RpcError::internal("storage.plugin.delete", e))?;
            Ok(Value::Null)
        }
        "storage.plugin.list" => {
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let namespace = host.plugin_storage_namespace(&ctx.plugin_id);
            let keys = storage::list(&conn, &namespace)
                .map_err(|e| RpcError::internal("storage.plugin.list", e))?;
            Ok(json!({ "keys": keys }))
        }
        _ => Err(RpcError::new(
            codes::NOT_FOUND,
            format!("unknown host method: {method}"),
        )),
    }
}

fn standalone_window_label(host: &PluginHost, ctx: &ConnectionContext) -> Result<String, RpcError> {
    let view_instance_id = ctx
        .view_instance_id()
        .ok_or_else(|| RpcError::new(codes::FORBIDDEN, "window APIs are UI-only"))?;
    let view = host
        .view(view_instance_id)
        .ok_or_else(|| RpcError::new(codes::NOT_FOUND, "view instance not found"))?;
    view.owner_window_label.ok_or_else(|| {
        RpcError::new(
            codes::FORBIDDEN,
            "window APIs are only available in standalone plugin windows",
        )
    })
}

fn parse_window_rect(params: &Value) -> Result<AppRect, RpcError> {
    let rect: AppRect = serde_json::from_value(params.clone())
        .map_err(|_| RpcError::new(codes::INVALID_REQUEST, "invalid window rect"))?;
    rect.validate()
        .map_err(|message| RpcError::new(codes::INVALID_REQUEST, message))?;
    Ok(rect)
}

fn require_str<'a>(params: &'a Value, field: &str) -> Result<&'a str, RpcError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, format!("{field} is required")))
}

fn require_app_state(app: &AppHandle) -> Result<tauri::State<'_, AppState>, RpcError> {
    app.try_state::<AppState>()
        .ok_or_else(|| RpcError::internal("plugin bridge", "app state unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_panel_methods_are_ui_only() {
        let ctx = ConnectionContext::runtime("com.example.hello");
        // Cheapest way to exercise the UI-only guard without a running Tauri app: assert the
        // context correctly reports no view instance for a Runtime-sourced connection.
        assert_eq!(ctx.view_instance_id(), None);

        let ui_ctx = ConnectionContext::ui("com.example.hello", "view-1");
        assert_eq!(ui_ctx.view_instance_id(), Some("view-1"));
    }

    #[test]
    fn rpc_error_serializes_with_expected_shape() {
        let error = RpcError::new(codes::NOT_FOUND, "unknown host method: foo.bar");
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "NOT_FOUND");
        assert_eq!(value["message"], "unknown host method: foo.bar");
        assert!(value.get("data").is_none());

        let with_data = RpcError {
            code: codes::COMMAND_FAILED.to_string(),
            message: "handler rejected input".into(),
            data: Some(json!({ "field": "query" })),
        };
        let value = serde_json::to_value(&with_data).unwrap();
        assert_eq!(value["code"], "COMMAND_FAILED");
        assert_eq!(value["data"]["field"], "query");
    }

    #[test]
    fn internal_errors_are_scrubbed() {
        let error = RpcError::internal(
            "storage.plugin.get",
            "sqlite: disk I/O error at /secret/path",
        );
        assert_eq!(error.code, codes::INTERNAL);
        assert!(!error.message.contains("secret"));
        assert!(!error.message.contains("sqlite"));
        assert_eq!(error.message, "storage.plugin.get failed");
    }

    #[test]
    fn payload_too_large_is_detected() {
        let huge = Value::String("x".repeat(MAX_MESSAGE_BYTES + 1));
        assert!(payload_too_large(&huge));
        assert!(!payload_too_large(&json!({ "ok": true })));
    }

    #[test]
    fn validates_standalone_window_rect_requests() {
        let rect = parse_window_rect(&json!({
            "width": "80%",
            "height": 600,
            "x": "center",
            "y": "10%"
        }))
        .unwrap();
        assert!(rect.width.is_some());
        assert!(rect.x.is_some());

        assert!(parse_window_rect(&json!({ "width": "120%" })).is_err());
        assert!(parse_window_rect(&json!({ "height": 200 })).is_err());
    }
}

/// Broadcast a theme change to every plugin subscription registered via `theme.onChange`.
/// Invoked from a global listener on the frontend-owned `settings:theme-changed` event.
pub fn broadcast_theme_change(app: &AppHandle, host: &PluginHost, theme: &str) {
    for (subscription_id, plugin_id, view_instance_id) in host.subscriptions_by_kind("theme") {
        let target_view = view_instance_id.unwrap_or_default();
        let _ = app.emit(
            "plugin-runtime-event",
            json!({
                "pluginId": plugin_id,
                "source": "platform",
                "viewInstanceId": target_view,
                "subscriptionId": subscription_id,
                "event": "theme.changed",
                "payload": { "theme": theme },
            }),
        );
    }
}
