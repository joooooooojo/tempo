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

    pub fn with_data(code: &str, message: impl Into<String>, data: Value) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            data: Some(data),
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

/// Single entry point for both UI (`plugin_bridge_invoke`) and Runtime-relayed `host.*` calls.
/// Routes `runtime.*` to the Supervisor (same plugin only); everything else to the Host API.
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
    if payload_too_large(&params) {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "request payload exceeds 1 MiB",
        ));
    }

    let _guard = acquire_slot(host, &ctx.plugin_id)?;

    if let Some(command_id) = method.strip_prefix("runtime.") {
        if command_id.is_empty() {
            return Err(RpcError::new(codes::INVALID_REQUEST, "missing runtime command id"));
        }
        return host
            .supervisor
            .call(&ctx.plugin_id, command_id, params, DEFAULT_TIMEOUT)
            .await;
    }

    let timeout = if matches!(method, "palette.setSize" | "palette.back" | "palette.hide") {
        INTERACTIVE_TIMEOUT
    } else {
        DEFAULT_TIMEOUT
    };

    tokio::time::timeout(timeout, dispatch_host_method(app, host, ctx, method, params))
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
        "palette.hide" => {
            crate::auxiliary_windows::hide_command_palette(app)
                .map_err(|e| RpcError::internal("palette.hide", e))?;
            Ok(Value::Null)
        }
        "palette.back" => {
            let Some(view_instance_id) = ctx.view_instance_id() else {
                return Err(RpcError::new(codes::FORBIDDEN, "palette.back is UI-only"));
            };
            app.emit(
                "plugin-host:palette-back",
                json!({ "viewInstanceId": view_instance_id }),
            )
            .map_err(|e| RpcError::internal("palette.back", e))?;
            Ok(Value::Null)
        }
        "palette.setSize" => {
            let Some(_view_instance_id) = ctx.view_instance_id() else {
                return Err(RpcError::new(codes::FORBIDDEN, "palette.setSize is UI-only"));
            };
            let height = params
                .get("height")
                .and_then(Value::as_f64)
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "height is required"))?;
            // Palette chrome keeps a fixed search width; plugins may only change height.
            crate::auxiliary_windows::set_command_palette_size(app.clone(), None, height)
                .map_err(|e| RpcError::internal("palette.setSize", e))?;
            Ok(Value::Null)
        }
        "app.open" => {
            let app_id = params
                .get("appId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "appId is required"))?;
            let open_params = params.get("params").cloned().unwrap_or(Value::Null);
            app.emit(
                "command-palette:open-app",
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
            if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
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
            use tauri_plugin_notification::NotificationExt;
            app.notification()
                .builder()
                .title(title)
                .body(body)
                .show()
                .map_err(|e| RpcError::internal("notify.show", e))?;
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
            host.register_subscription(&subscription_id, &ctx.plugin_id, "theme", ctx.view_instance_id());
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
                .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, "subscriptionId is required"))?;
            host.release_subscription(subscription_id, &ctx.plugin_id);
            Ok(Value::Null)
        }
        "storage.plugin.get" => {
            let key = require_str(&params, "key")?;
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let value = storage::get(&conn, &ctx.plugin_id, key).map_err(|e| RpcError::internal("storage.plugin.get", e))?;
            Ok(json!({ "value": value }))
        }
        "storage.plugin.set" => {
            let key = require_str(&params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            storage::set(&conn, &ctx.plugin_id, key, &value).map_err(|e| RpcError::new(codes::RESOURCE_EXHAUSTED, e))?;
            Ok(Value::Null)
        }
        "storage.plugin.delete" => {
            let key = require_str(&params, "key")?;
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            storage::delete(&conn, &ctx.plugin_id, key).map_err(|e| RpcError::internal("storage.plugin.delete", e))?;
            Ok(Value::Null)
        }
        "storage.plugin.list" => {
            let state = require_app_state(app)?;
            let conn = state.db.lock();
            let keys = storage::list(&conn, &ctx.plugin_id).map_err(|e| RpcError::internal("storage.plugin.list", e))?;
            Ok(json!({ "keys": keys }))
        }
        _ => Err(RpcError::new(
            codes::NOT_FOUND,
            format!("unknown host method: {method}"),
        )),
    }
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
    fn palette_methods_are_ui_only() {
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
    }

    #[test]
    fn internal_errors_are_scrubbed() {
        let error = RpcError::internal("storage.plugin.get", "sqlite: disk I/O error at /secret/path");
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
                "viewInstanceId": target_view,
                "subscriptionId": subscription_id,
                "event": "theme.changed",
                "payload": { "theme": theme },
            }),
        );
    }
}
