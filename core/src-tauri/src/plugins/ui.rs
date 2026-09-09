//! Plugin UI surface: the `tempo-plugin://` resource protocol, plugin asset URL builders, and
//! session persistence helpers backing `PluginAppHost` (design §5, decision: iframe host over
//! a custom protocol rather than a raw Wry child WebView — see task architecture notes).

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::http::{
    header::CACHE_CONTROL,
    header::{CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE},
    Method, Request, Response, StatusCode,
};
use tauri::{AppHandle, Manager};

use crate::asset_protocol::{percent_decode, percent_encode};

use super::host::{DevelopmentUiSource, PluginHost};
use super::manifest::PluginManifest;

pub const PROTOCOL: &str = "tempo-plugin";

/// Host-owned bridge script path (not read from the plugin package). Injected into every
/// plugin HTML document so authors get `window.tempo` and `window.ipcRenderer` directly.
pub const BRIDGE_CLIENT_PATH: &str = "__tempo__/client.js";
pub const BRIDGE_SCA_PATH: &str = "__tempo__/structured-clone.js";

const BRIDGE_SCA_SOURCE: &str = include_str!("../../../plugin-ui/structured-clone.js");
const BRIDGE_CLIENT_SOURCE: &str = include_str!("../../../plugin-ui/bridge-client.js");
const BRIDGE_SCRIPT_TAG: &str = concat!(
    r#"<script src="__tempo__/structured-clone.js"></script>"#,
    r#"<script src="__tempo__/client.js"></script>"#,
);

/// Baseline CSP (design §5.2). The Runtime `net` permission also opens managed UI network
/// access. Remote scripts, styles, frames, and forms remain disabled.
fn csp_for_permissions(permissions: &super::permissions::PluginPermissions) -> String {
    if permissions.allows_global(super::permissions::PluginPermission::Net) {
        return "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob: http: https:; media-src 'self' blob: http: https:; \
font-src 'self' data: http: https:; connect-src 'self' http: https: ws: wss:; \
object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'"
            .into();
    }
    let mut web_sources = Vec::new();
    let mut connect_sources = Vec::new();
    for endpoint in permissions.legacy_net_endpoints().unwrap_or_default() {
        let Some(authority) = canonical_net_authority(endpoint) else {
            continue;
        };
        let http = format!("http://{authority}");
        let https = format!("https://{authority}");
        web_sources.extend([http.clone(), https.clone()]);
        connect_sources.extend([
            http,
            https,
            format!("ws://{authority}"),
            format!("wss://{authority}"),
        ]);
    }

    let web_suffix = if web_sources.is_empty() {
        String::new()
    } else {
        format!(" {}", web_sources.join(" "))
    };
    let connect_suffix = if connect_sources.is_empty() {
        String::new()
    } else {
        format!(" {}", connect_sources.join(" "))
    };
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:{web_suffix}; media-src 'self' blob:{web_suffix}; \
font-src 'self' data:{web_suffix}; connect-src 'self'{connect_suffix}; \
object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'"
    )
}

fn canonical_net_authority(endpoint: &str) -> Option<String> {
    let parsed = url::Url::parse(&format!("http://{endpoint}")).ok()?;
    let host = match parsed.host()? {
        url::Host::Domain(domain) => domain.to_string(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    let (_, port) = endpoint.rsplit_once(':')?;
    Some(format!("{host}:{}", port.parse::<u16>().ok()?))
}

/// Stable, collision-resistant identifier for a plugin usable in a URL path segment. Not a
/// secret — only used so plugin resource URLs don't leak the raw reverse-DNS id verbatim and so
/// the protocol handler has an explicit hash->id allowlist to check against (design §5.2).
pub fn plugin_hash_of(plugin_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plugin_id.as_bytes());
    hex::encode(hasher.finalize())
}

fn build_url(plugin_hash: &str, rel_path: &str) -> String {
    let encoded_segments = rel_path
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    if cfg!(target_os = "windows") {
        format!("http://{PROTOCOL}.localhost/{plugin_hash}/{encoded_segments}")
    } else {
        format!("{PROTOCOL}://localhost/{plugin_hash}/{encoded_segments}")
    }
}

fn build_view_url(view_instance_id: &str, plugin_hash: &str, rel_path: &str) -> String {
    let encoded_segments = rel_path
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    if cfg!(target_os = "windows") {
        format!("http://{PROTOCOL}.localhost/{view_instance_id}/{plugin_hash}/{encoded_segments}")
    } else {
        format!("{PROTOCOL}://localhost/{view_instance_id}/{plugin_hash}/{encoded_segments}")
    }
}

pub fn plugin_entry_url(plugin_hash: &str, rel_path: &str) -> String {
    build_url(plugin_hash, rel_path)
}

/// URL used by a mounted plugin view. The view instance ID is a capability: the protocol
/// handler verifies that it belongs to the plugin hash in the request before serving anything.
pub fn plugin_view_entry_url(view_instance_id: &str, plugin_hash: &str, rel_path: &str) -> String {
    build_view_url(view_instance_id, plugin_hash, rel_path)
}

pub fn plugin_icon_url(plugin_hash: &str, rel_path: &str) -> String {
    build_url(plugin_hash, rel_path)
}

/// Parse either a public asset path `/{pluginHash}/{relpath...}` or a view capability path
/// `/{viewInstanceId}/{pluginHash}/{relpath...}` from the incoming request path.
fn parse_request_path(path: &str) -> Option<(Option<String>, String, String)> {
    let trimmed = path.trim_start_matches('/');
    let (first, rest) = trimmed.split_once('/')?;
    if first.is_empty() || rest.is_empty() {
        return None;
    }
    let (view_instance_id, hash, rel_encoded) = if first.starts_with("view-") {
        let (hash, rel_encoded) = rest.split_once('/')?;
        (Some(first.to_string()), hash.to_string(), rel_encoded)
    } else {
        (None, first.to_string(), rest)
    };
    if hash.is_empty() || rel_encoded.is_empty() {
        return None;
    }
    let rel = rel_encoded
        .split('/')
        .map(percent_decode)
        .collect::<Vec<_>>()
        .join("/");
    Some((view_instance_id, hash, rel))
}

fn content_type_for(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "text/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if let Some(mime) = super::icons::mime_type_for_path(Path::new(name)) {
        mime
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else {
        "application/octet-stream"
    }
}

fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap()
}

fn ok_bytes(
    content_type: &str,
    bytes: Vec<u8>,
    head_only: bool,
    no_store: bool,
    csp: &str,
) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(CONTENT_SECURITY_POLICY, csp);
    if no_store {
        builder = builder.header(CACHE_CONTROL, "no-store");
    }
    if !head_only {
        builder = builder.header(CONTENT_LENGTH, bytes.len());
    }
    builder
        .body(if head_only { Vec::new() } else { bytes })
        .unwrap()
}

fn is_reserved_host_path(rel_path: &str) -> bool {
    rel_path == BRIDGE_CLIENT_PATH || rel_path.starts_with("__tempo__/")
}

fn is_public_asset_path(rel_path: &str) -> bool {
    let content_type = content_type_for(rel_path);
    content_type.starts_with("image/") || content_type.starts_with("font/")
}

/// Insert the host bridge first so injected globals exist before plugin code runs.
fn inject_bridge_script(html: &[u8]) -> Vec<u8> {
    if html
        .windows(BRIDGE_SCRIPT_TAG.len())
        .any(|w| w == BRIDGE_SCRIPT_TAG.as_bytes())
    {
        return html.to_vec();
    }
    let script = BRIDGE_SCRIPT_TAG.as_bytes();
    if let Some(idx) = find_ascii_tag(html, b"<head") {
        if let Some(end) = html[idx..].iter().position(|&b| b == b'>') {
            let insert_at = idx + end + 1;
            let mut out = Vec::with_capacity(html.len() + script.len());
            out.extend_from_slice(&html[..insert_at]);
            out.extend_from_slice(script);
            out.extend_from_slice(&html[insert_at..]);
            return out;
        }
    }
    let mut out = Vec::with_capacity(script.len() + html.len());
    out.extend_from_slice(script);
    out.extend_from_slice(html);
    out
}

fn find_ascii_tag(haystack: &[u8], tag: &[u8]) -> Option<usize> {
    haystack.windows(tag.len()).position(|window| {
        window
            .iter()
            .zip(tag.iter())
            .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
    })
}

/// `register_uri_scheme_protocol` handler for `tempo-plugin://`. Only serves files inside a
/// registered (enabled + trusted, scanned by the loader) plugin's install directory; every
/// request is re-validated against a canonicalized root regardless of what the loader already
/// checked (defense in depth against path traversal / symlink tricks, design §5.2).
pub fn protocol_response(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED);
    }
    let head_only = request.method() == Method::HEAD;

    let Some((view_instance_id, plugin_hash, rel_path)) = parse_request_path(request.uri().path())
    else {
        return empty_response(StatusCode::BAD_REQUEST);
    };

    let Some(host) = app.try_state::<std::sync::Arc<PluginHost>>() else {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let Some(entry) = host.resolve_hash(&plugin_hash) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let development = entry.package_hash.is_empty();
    let csp = csp_for_permissions(&entry.permissions);

    if let Some(view_instance_id) = view_instance_id {
        let Some(view) = host.view(&view_instance_id) else {
            return empty_response(StatusCode::FORBIDDEN);
        };
        if view.plugin_id != entry.plugin_id {
            return empty_response(StatusCode::FORBIDDEN);
        }
    } else if !is_public_asset_path(&rel_path) {
        // Documents, scripts, styles, and the bridge must use a view capability so one plugin
        // cannot address another by hash. Legacy public URLs remain for media only.
        return empty_response(StatusCode::FORBIDDEN);
    }

    // Host-owned bridge — never read from the plugin package (reserved `__tempo__/` namespace).
    if rel_path == BRIDGE_SCA_PATH {
        return ok_bytes(
            "text/javascript; charset=utf-8",
            BRIDGE_SCA_SOURCE.as_bytes().to_vec(),
            head_only,
            development,
            &csp,
        );
    }
    if rel_path == BRIDGE_CLIENT_PATH {
        return ok_bytes(
            "text/javascript; charset=utf-8",
            BRIDGE_CLIENT_SOURCE.as_bytes().to_vec(),
            head_only,
            development,
            &csp,
        );
    }
    if is_reserved_host_path(&rel_path) {
        return empty_response(StatusCode::FORBIDDEN);
    }

    let asset_roots = development_asset_roots(&host, &entry);
    let Some((canonical_root, canonical_path)) = resolve_asset_path(&asset_roots, &rel_path) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return empty_response(StatusCode::FORBIDDEN);
    }

    let content_type = content_type_for(&rel_path);

    if head_only {
        return ok_bytes(content_type, Vec::new(), true, development, &csp);
    }

    match std::fs::read(&canonical_path) {
        Ok(bytes) => {
            let body = if content_type.starts_with("text/html") {
                inject_bridge_script(&bytes)
            } else {
                bytes
            };
            ok_bytes(content_type, body, false, development, &csp)
        }
        Err(error) => {
            tracing::debug!(plugin_id = %entry.plugin_id, error = %error, "failed to read plugin asset");
            empty_response(StatusCode::NOT_FOUND)
        }
    }
}

fn development_asset_roots(
    host: &PluginHost,
    entry: &super::host::PluginRegistryEntry,
) -> Vec<PathBuf> {
    let mut roots = vec![entry.install_path.clone()];
    if !entry.package_hash.is_empty() {
        return roots;
    }
    let Some(development) = host.development_plugin(&entry.plugin_id) else {
        return roots;
    };
    push_unique_path(&mut roots, development.root_path.clone());
    if let Some(DevelopmentUiSource::Static(path)) = development.ui_source {
        push_unique_path(&mut roots, path);
    }
    roots
}

fn push_unique_path(roots: &mut Vec<PathBuf>, path: PathBuf) {
    if roots.iter().any(|existing| existing == &path) {
        return;
    }
    roots.push(path);
}

fn resolve_asset_path(roots: &[PathBuf], rel_path: &str) -> Option<(PathBuf, PathBuf)> {
    let relative = PathBuf::from(rel_path);
    for root in roots {
        let Ok(canonical_root) = root.canonicalize() else {
            continue;
        };
        let candidate = root.join(&relative);
        let Ok(canonical_path) = candidate.canonicalize() else {
            continue;
        };
        if canonical_path.starts_with(&canonical_root) && canonical_path.is_file() {
            return Some((canonical_root, canonical_path));
        }
    }
    None
}

/// Load `manifest.json` from an install directory (used by `plugin_ui_prepare` to resolve an
/// app's `entry`/`rect` before minting a view instance).
pub fn read_manifest(install_path: &Path) -> Result<PluginManifest, String> {
    let raw = std::fs::read_to_string(install_path.join("manifest.json"))
        .map_err(|e| format!("read manifest.json: {e}"))?;
    PluginManifest::parse_str(&raw)
}

// -- Session persistence (design §5.5) --------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSessionEnvelope {
    pub plugin_id: String,
    pub app_id: String,
    pub plugin_version: String,
    pub session_version: u32,
    pub payload: serde_json::Value,
    pub updated_at: String,
}

/// Returns the persisted session only if the plugin version + `sessionVersion` still match —
/// stale payloads are dropped rather than handed to the plugin (design §4.3, §5.5).
pub fn load_session(
    conn: &Connection,
    plugin_id: &str,
    app_id: &str,
    current_version: &str,
    current_session_version: u32,
) -> Result<Option<PluginSessionEnvelope>, String> {
    let row: Option<(String, i64, String, String)> = conn
        .query_row(
            "SELECT plugin_version, session_version, payload, updated_at
             FROM plugin_sessions WHERE plugin_id = ?1 AND app_id = ?2",
            params![plugin_id, app_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|e| format!("load plugin session: {e}"))?;

    let Some((plugin_version, session_version, payload_raw, updated_at)) = row else {
        return Ok(None);
    };
    if plugin_version != current_version || session_version as u32 != current_session_version {
        clear_session(conn, plugin_id, app_id)?;
        return Ok(None);
    }
    let payload = serde_json::from_str(&payload_raw).unwrap_or(serde_json::Value::Null);
    Ok(Some(PluginSessionEnvelope {
        plugin_id: plugin_id.to_string(),
        app_id: app_id.to_string(),
        plugin_version,
        session_version: session_version as u32,
        payload,
        updated_at,
    }))
}

/// 64 KiB session payload cap (design §5.5): oversized payloads are dropped, not truncated.
pub const MAX_SESSION_PAYLOAD_BYTES: usize = 64 * 1024;

pub fn save_session(
    conn: &Connection,
    plugin_id: &str,
    app_id: &str,
    plugin_version: &str,
    session_version: u32,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let serialized =
        serde_json::to_string(payload).map_err(|e| format!("serialize session: {e}"))?;
    if serialized.len() > MAX_SESSION_PAYLOAD_BYTES {
        return Err(format!(
            "session payload exceeds {MAX_SESSION_PAYLOAD_BYTES} bytes"
        ));
    }
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_sessions (plugin_id, app_id, plugin_version, session_version, payload, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(plugin_id, app_id) DO UPDATE SET
           plugin_version = excluded.plugin_version,
           session_version = excluded.session_version,
           payload = excluded.payload,
           updated_at = excluded.updated_at",
        params![plugin_id, app_id, plugin_version, session_version, serialized, now],
    )
    .map_err(|e| format!("save plugin session: {e}"))?;
    Ok(())
}

pub fn clear_session(conn: &Connection, plugin_id: &str, app_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM plugin_sessions WHERE plugin_id = ?1 AND app_id = ?2",
        params![plugin_id, app_id],
    )
    .map_err(|e| format!("clear plugin session: {e}"))?;
    Ok(())
}

pub fn clear_all_sessions_for_plugin(conn: &Connection, plugin_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM plugin_sessions WHERE plugin_id = ?1",
        params![plugin_id],
    )
    .map_err(|e| format!("clear plugin sessions: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_bridge_after_head() {
        let html = b"<html><head><title>x</title></head><body></body></html>";
        let out = String::from_utf8(inject_bridge_script(html)).unwrap();
        assert!(out.contains(BRIDGE_SCRIPT_TAG));
        assert!(out.find("<head>").unwrap() < out.find(BRIDGE_SCRIPT_TAG).unwrap());
        assert_eq!(inject_bridge_script(out.as_bytes()), out.as_bytes());
    }

    #[test]
    fn development_resources_disable_browser_cache() {
        let csp = csp_for_permissions(&Default::default());
        let response = ok_bytes("text/javascript", b"export {};".to_vec(), false, true, &csp);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    }

    #[test]
    fn ui_network_is_denied_without_a_manifest_grant() {
        let csp = csp_for_permissions(&Default::default());
        assert!(csp.contains("connect-src 'self';"));
        assert!(csp.contains("img-src 'self' data: blob:;"));
        assert!(!csp.contains("connect-src https:"));
        assert!(!csp.contains("img-src 'self' data: blob: https:"));
    }

    #[test]
    fn legacy_ui_network_uses_the_runtime_host_port_grants() {
        let permissions: super::super::permissions::PluginPermissions =
            serde_json::from_str(r#"{"net":["api.example.com:443","[::1]:8080"]}"#).unwrap();
        let csp = csp_for_permissions(&permissions);
        for source in [
            "https://api.example.com:443",
            "wss://api.example.com:443",
            "http://[::1]:8080",
            "ws://[::1]:8080",
        ] {
            assert!(csp.contains(source), "missing {source} in {csp}");
        }
        assert!(csp.contains("script-src 'self';"));
        assert!(csp.contains("frame-src 'none';"));
    }

    #[test]
    fn net_permission_opens_ui_network_without_enabling_remote_code() {
        let permissions: super::super::permissions::PluginPermissions =
            serde_json::from_str(r#"["net"]"#).unwrap();
        let csp = csp_for_permissions(&permissions);
        assert!(csp.contains("connect-src 'self' http: https: ws: wss:;"));
        assert!(csp.contains("img-src 'self' data: blob: http: https:;"));
        assert!(csp.contains("script-src 'self';"));
        assert!(csp.contains("frame-src 'none';"));
    }

    #[test]
    fn view_urls_include_a_capability_segment() {
        let url = plugin_view_entry_url("view-secret", "plugin-hash", "dist/index.html");
        let path = url::Url::parse(&url).unwrap().path().to_string();
        assert_eq!(
            parse_request_path(&path),
            Some((
                Some("view-secret".into()),
                "plugin-hash".into(),
                "dist/index.html".into(),
            ))
        );
    }

    #[test]
    fn public_asset_urls_preserve_nested_relative_paths() {
        let url = plugin_icon_url("plugin-hash", "icons/app.svg");
        let path = url::Url::parse(&url).unwrap().path().to_string();
        assert_eq!(
            parse_request_path(&path),
            Some((None, "plugin-hash".into(), "icons/app.svg".into()))
        );
    }

    #[test]
    fn legacy_plugin_urls_are_public_only_for_media() {
        for icon in [
            "icon.svg",
            "icon.png",
            "icon.jpg",
            "icon.jpeg",
            "icon.webp",
            "icon.gif",
        ] {
            assert!(is_public_asset_path(icon), "{icon} should be public");
        }
        assert!(is_public_asset_path("fonts/inter.woff2"));
        assert!(!is_public_asset_path("icon.ico"));
        assert!(!is_public_asset_path("index.html"));
        assert!(!is_public_asset_path("dist/index.js"));
    }
}
