//! Git-backed plugin repositories: source configuration, credentials, catalog sync, and install.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use git2::{
    CertificateCheckStatus, Cred, CredentialType, FetchOptions, ObjectType, ProxyOptions,
    RemoteCallbacks, Repository,
};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use semver::{Version, VersionReq};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::db::AppState;

use super::bridge::HOST_API_VERSION;
use super::icons;
use super::ids::{is_valid_plugin_id, is_valid_repository_index_id};
use super::manifest::{current_host_platform, PluginManifest};
use super::package::{import_directory, inspect_package};
use super::paths::{ensure_dir, repository_cache_dir, staging_dir};
use super::trust::{ensure_plugin_tables, record_installed_version};

const INDEX_FILE: &str = "tempo-plugin-repository.json";
const INDEX_MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_REPOSITORY_PLUGINS: usize = 5_000;
const MAX_GIT_FILES: usize = 10_000;
const MAX_GIT_FILE_BYTES: usize = 200 * 1024 * 1024;
const MAX_GIT_PACKAGE_BYTES: usize = 500 * 1024 * 1024;
const MAX_GIT_TRANSFER_BYTES: usize = 1024 * 1024 * 1024;
const GIT_FETCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const OPERATION_EVENT: &str = "plugin-repository-operation-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRepository {
    pub id: String,
    pub url: String,
    pub transport: String,
    pub git_ref: String,
    pub index_path: String,
    pub authentication_mode: String,
    pub credential_id: Option<String>,
    pub credential_status: String,
    pub allow_insecure_transport: bool,
    pub allow_insecure_credentials: bool,
    pub display_name: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub priority: i64,
    pub snapshot_commit: Option<String>,
    pub valid_plugin_count: usize,
    pub issue_count: usize,
    pub connection_verified_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub has_cached_catalog: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRepositoryInput {
    pub url: String,
    #[serde(default = "default_git_ref")]
    pub git_ref: String,
    #[serde(default = "default_index_path")]
    pub index_path: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub authentication_mode: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
    #[serde(default)]
    pub allow_insecure_transport: bool,
    #[serde(default)]
    pub allow_insecure_credentials: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepositoryInput {
    pub repository_id: String,
    pub url: String,
    #[serde(default = "default_git_ref")]
    pub git_ref: String,
    #[serde(default = "default_index_path")]
    pub index_path: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub authentication_mode: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
    #[serde(default)]
    pub allow_insecure_transport: bool,
    #[serde(default)]
    pub allow_insecure_credentials: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCredentialProfile {
    pub id: String,
    pub display_name: String,
    pub scope_origin: String,
    pub auth_kind: String,
    pub username: Option<String>,
    pub ssh_private_key_path: Option<String>,
    pub secret_storage: String,
    pub available: bool,
    pub referenced_repository_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCredentialInput {
    #[serde(default)]
    pub id: Option<String>,
    pub display_name: String,
    pub scope_url: String,
    pub auth_kind: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub ssh_private_key_path: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCatalogPlugin {
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub version: String,
    pub package_hash: String,
    pub source_commit: String,
    pub repository_id: String,
    pub repository_name: String,
    pub compatible: bool,
    pub incompatible_reason: Option<String>,
    pub installed_version: Option<String>,
    pub pending_version: Option<String>,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIssue {
    pub repository_id: String,
    pub plugin_id: Option<String>,
    pub plugin_root: Option<String>,
    pub error: String,
    pub source_commit: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryOperation {
    pub operation_id: String,
    pub kind: String,
    pub repository_id: String,
    pub plugin_id: Option<String>,
    pub status: String,
    pub phase: String,
    pub message: String,
    pub transferred_bytes: u64,
    pub total_items: Option<usize>,
    pub completed_items: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStarted {
    pub operation_id: String,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryTrustChallenge {
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub fingerprint_sha256: String,
    pub key_type: Option<String>,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub not_after: Option<String>,
    pub confirmation_nonce: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryConnectionTest {
    pub repository_id: String,
    pub status: String,
    pub message: String,
    pub challenge: Option<RepositoryTrustChallenge>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustRepositoryConnectionInput {
    pub confirmation_nonce: String,
    pub host: String,
    pub port: u16,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryIndex {
    /// Editor tooling only. Tempo ignores this field.
    #[serde(rename = "$schema", default)]
    _json_schema: Option<String>,
    schema_version: u32,
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    plugins: Vec<RepositoryIndexPlugin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryIndexPlugin {
    id: String,
    path: String,
}

#[derive(Debug, Clone)]
struct RepositoryConfig {
    id: String,
    url: String,
    transport: String,
    git_ref: String,
    index_path: String,
    authentication_mode: String,
    credential_id: Option<String>,
    allow_insecure_transport: bool,
    allow_insecure_credentials: bool,
    proxy_mode: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct CredentialRecord {
    id: String,
    scope_scheme: String,
    scope_host: String,
    scope_port: u16,
    auth_kind: String,
    username: Option<String>,
    ssh_private_key_path: Option<String>,
    secret_storage: String,
}

#[derive(Debug, Clone)]
enum ResolvedCredential {
    Http {
        username: String,
        secret: Zeroizing<String>,
    },
    SshAgent {
        username: String,
    },
    SshKey {
        username: String,
        private_key: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
}

#[derive(Debug, Clone)]
struct ParsedRemote {
    normalized_url: String,
    transport: String,
    host: String,
    port: u16,
    username: Option<String>,
}

#[derive(Debug)]
struct CatalogRecord {
    plugin_id: String,
    plugin_root: String,
    manifest: PluginManifest,
    package_hash: String,
    icon_data_url: Option<String>,
    compatible: bool,
    incompatible_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TransportTrust {
    ssh_host_keys: HashSet<(String, String)>,
}

#[derive(Debug, Clone)]
struct PendingTrustChallenge {
    challenge: RepositoryTrustChallenge,
    created_at: Instant,
}

enum CertificateOutcome {
    TrustRequired(RepositoryTrustChallenge),
    Changed(String),
}

enum FetchRepositoryError {
    Message(String),
    TrustRequired(RepositoryTrustChallenge),
}

impl From<String> for FetchRepositoryError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl std::fmt::Display for FetchRepositoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::TrustRequired(challenge) => write!(
                formatter,
                "SSH 主机密钥 {}:{} 尚未信任（{}）",
                challenge.host, challenge.port, challenge.fingerprint_sha256
            ),
        }
    }
}

struct StrictJsonValue(serde_json::Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StrictJsonVisitor;

        impl<'de> Visitor<'de> for StrictJsonVisitor {
            type Value = StrictJsonValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(serde_json::Value::Number)
                    .map(StrictJsonValue)
                    .ok_or_else(|| E::custom("JSON number is not finite"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::String(
                    value.to_string(),
                )))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Null))
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON object key: {key}"
                        )));
                    }
                    let value = map.next_value::<StrictJsonValue>()?;
                    values.insert(key, value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Object(values)))
            }
        }

        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

static OPERATIONS: OnceLock<Mutex<HashMap<String, RepositoryOperation>>> = OnceLock::new();
static SESSION_SECRETS: OnceLock<Mutex<HashMap<String, Zeroizing<String>>>> = OnceLock::new();
static GIT_NETWORK_OPTIONS: OnceLock<Result<(), String>> = OnceLock::new();
static PENDING_TRUST_CHALLENGES: OnceLock<Mutex<HashMap<String, PendingTrustChallenge>>> =
    OnceLock::new();

fn operations() -> &'static Mutex<HashMap<String, RepositoryOperation>> {
    OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_secrets() -> &'static Mutex<HashMap<String, Zeroizing<String>>> {
    SESSION_SECRETS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_trust_challenges() -> &'static Mutex<HashMap<String, PendingTrustChallenge>> {
    PENDING_TRUST_CHALLENGES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn default_git_ref() -> String {
    "HEAD".into()
}

fn default_index_path() -> String {
    INDEX_FILE.into()
}

fn validate_display_name(value: Option<&str>) -> Result<Option<String>, String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    if value.is_some_and(|value| value.chars().count() > 128) {
        return Err("仓库显示名称不能超过 128 个字符".into());
    }
    Ok(value.map(str::to_string))
}

fn has_active_repository_operation(repository_id: &str) -> bool {
    operations().lock().values().any(|operation| {
        operation.repository_id == repository_id
            && matches!(operation.status.as_str(), "queued" | "running")
    })
}

fn ensure_git_network_options() -> Result<(), String> {
    GIT_NETWORK_OPTIONS
        .get_or_init(|| unsafe {
            git2::opts::set_server_connect_timeout_in_milliseconds(15_000)
                .and_then(|_| git2::opts::set_server_timeout_in_milliseconds(300_000))
                .map_err(|error| format!("配置 Git 网络超时失败: {error}"))
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

fn generate_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}")
}

pub fn ensure_repository_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS plugin_repository_credentials (
          id TEXT PRIMARY KEY,
          display_name TEXT NOT NULL,
          scope_scheme TEXT NOT NULL,
          scope_host TEXT NOT NULL,
          scope_port INTEGER NOT NULL,
          auth_kind TEXT NOT NULL,
          username TEXT,
          ssh_private_key_path TEXT,
          secret_storage TEXT NOT NULL,
          secret_locator TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS plugin_repositories (
          id TEXT PRIMARY KEY,
          url TEXT NOT NULL,
          transport TEXT NOT NULL,
          git_ref TEXT NOT NULL DEFAULT 'HEAD',
          index_path TEXT NOT NULL DEFAULT 'tempo-plugin-repository.json',
          authentication_mode TEXT NOT NULL DEFAULT 'anonymous',
          credential_id TEXT,
          allow_insecure_transport INTEGER NOT NULL DEFAULT 0,
          allow_insecure_credentials INTEGER NOT NULL DEFAULT 0,
          proxy_mode TEXT NOT NULL DEFAULT 'system',
          display_name TEXT,
          remote_repository_id TEXT,
          remote_name TEXT,
          remote_description TEXT,
          remote_homepage TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          priority INTEGER NOT NULL,
          snapshot_commit TEXT,
          valid_plugin_count INTEGER NOT NULL DEFAULT 0,
          issue_count INTEGER NOT NULL DEFAULT 0,
          connection_verified_at TEXT,
          last_sync_at TEXT,
          last_success_at TEXT,
          last_error TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(url, git_ref, index_path),
          FOREIGN KEY(credential_id) REFERENCES plugin_repository_credentials(id) ON DELETE SET NULL
        );
        CREATE TABLE IF NOT EXISTS plugin_repository_plugins (
          repository_id TEXT NOT NULL,
          plugin_id TEXT NOT NULL,
          plugin_root TEXT NOT NULL,
          version TEXT NOT NULL,
          package_hash TEXT NOT NULL,
          name TEXT NOT NULL,
          publisher TEXT,
          description TEXT,
          kind TEXT NOT NULL,
          categories_json TEXT NOT NULL DEFAULT '[]',
          platforms_json TEXT NOT NULL DEFAULT '[]',
          engine_tempo TEXT NOT NULL,
          engine_plugin_api TEXT NOT NULL,
          requires_node_runtime INTEGER NOT NULL DEFAULT 0,
          icon_data_url TEXT,
          compatible INTEGER NOT NULL DEFAULT 0,
          incompatible_reason TEXT,
          source_commit TEXT NOT NULL,
          indexed_at TEXT NOT NULL,
          PRIMARY KEY(repository_id, plugin_id),
          FOREIGN KEY(repository_id) REFERENCES plugin_repositories(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS plugin_repository_issues (
          repository_id TEXT NOT NULL,
          entry_key TEXT NOT NULL,
          declared_plugin_id TEXT,
          plugin_root TEXT,
          error TEXT NOT NULL,
          source_commit TEXT NOT NULL,
          created_at TEXT NOT NULL,
          PRIMARY KEY(repository_id, entry_key),
          FOREIGN KEY(repository_id) REFERENCES plugin_repositories(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS plugin_repository_tls_pins (
          host TEXT NOT NULL,
          port INTEGER NOT NULL,
          fingerprint_sha256 TEXT NOT NULL,
          subject TEXT,
          issuer TEXT,
          not_after TEXT,
          trusted_at TEXT NOT NULL,
          PRIMARY KEY(host, port)
        );
        CREATE TABLE IF NOT EXISTS plugin_repository_ssh_host_keys (
          host TEXT NOT NULL,
          port INTEGER NOT NULL,
          key_type TEXT NOT NULL,
          fingerprint_sha256 TEXT NOT NULL,
          trusted_at TEXT NOT NULL,
          last_seen_at TEXT NOT NULL,
          PRIMARY KEY(host, port, key_type, fingerprint_sha256)
        );
        CREATE INDEX IF NOT EXISTS idx_plugin_repositories_priority
          ON plugin_repositories(enabled, priority, id);
        CREATE INDEX IF NOT EXISTS idx_repository_plugins_plugin_id
          ON plugin_repository_plugins(plugin_id, repository_id);
        ",
    )
    .map_err(|error| format!("create plugin repository tables: {error}"))?;
    ensure_column(conn, "plugin_versions", "source_repository_id", "TEXT")?;
    ensure_column(conn, "plugin_versions", "source_commit", "TEXT")?;
    ensure_column(conn, "plugin_versions", "source_plugin_root", "TEXT")?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("inspect {table}: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("read {table} columns: {error}"))?;
    for current in columns {
        if current.map_err(|error| error.to_string())? == column {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| format!("add {table}.{column}: {error}"))?;
    Ok(())
}

fn parse_remote(input: &str) -> Result<ParsedRemote, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("请输入 Git 仓库地址".into());
    }
    let normalized = if !raw.contains("://") {
        let (user_host, path) = raw
            .split_once(':')
            .ok_or_else(|| "仓库地址必须使用 HTTP(S)、SSH 或 SCP 格式".to_string())?;
        if path.is_empty() || user_host.is_empty() {
            return Err("无效的 SCP 风格 Git 地址".into());
        }
        format!("ssh://{user_host}/{}", path.trim_start_matches('/'))
    } else {
        raw.to_string()
    };
    let parsed =
        url::Url::parse(&normalized).map_err(|error| format!("无效的 Git 地址: {error}"))?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https" | "ssh") {
        return Err("仅支持 HTTP(S) 和 SSH Git 仓库".into());
    }
    if parsed.fragment().is_some() {
        return Err("Git 地址不能包含 fragment".into());
    }
    if parsed.query().is_some() {
        return Err("Git 地址不能包含查询参数，请使用凭证配置 Token 或密码".into());
    }
    if matches!(scheme, "http" | "https")
        && (!parsed.username().is_empty() || parsed.password().is_some())
    {
        return Err("不要在 Git 地址中填写用户名或 Token，请使用凭证配置".into());
    }
    if scheme == "ssh" && parsed.password().is_some() {
        return Err("不要在 SSH 地址中填写密码，请使用凭证配置".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Git 地址缺少主机名".to_string())?
        .to_ascii_lowercase();
    let port = parsed
        .port_or_known_default()
        .unwrap_or(if scheme == "ssh" { 22 } else { 443 });
    let username = if scheme == "ssh" && !parsed.username().is_empty() {
        Some(parsed.username().to_string())
    } else {
        None
    };
    Ok(ParsedRemote {
        normalized_url: parsed.to_string(),
        transport: scheme.to_string(),
        host,
        port,
        username,
    })
}

fn validate_index_path(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || value.contains('\0')
        || value.contains(':')
        || value.nfc().collect::<String>() != value
    {
        return Err("索引路径必须是仓库内的安全相对路径".into());
    }
    Ok(value.to_string())
}

fn fingerprint_sha256(bytes: &[u8]) -> String {
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(bytes))
    )
}

fn ssh_challenge(
    remote: &ParsedRemote,
    key_type: &str,
    fingerprint: String,
) -> RepositoryTrustChallenge {
    RepositoryTrustChallenge {
        kind: "ssh-host-key".into(),
        host: remote.host.clone(),
        port: remote.port,
        fingerprint_sha256: fingerprint,
        key_type: Some(key_type.to_string()),
        subject: None,
        issuer: None,
        not_after: None,
        confirmation_nonce: generate_id("trust"),
    }
}

fn load_transport_trust(
    conn: &Connection,
    remote: &ParsedRemote,
) -> Result<TransportTrust, String> {
    let mut statement = conn
        .prepare(
            "SELECT key_type, fingerprint_sha256 FROM plugin_repository_ssh_host_keys
             WHERE host = ?1 AND port = ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![remote.host, remote.port as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let ssh_host_keys = rows
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    Ok(TransportTrust { ssh_host_keys })
}

fn remember_pending_trust(challenge: RepositoryTrustChallenge) {
    let mut pending = pending_trust_challenges().lock();
    pending.retain(|_, value| value.created_at.elapsed() <= Duration::from_secs(5 * 60));
    pending.insert(
        challenge.confirmation_nonce.clone(),
        PendingTrustChallenge {
            challenge,
            created_at: Instant::now(),
        },
    );
}

fn consume_pending_trust(
    input: &TrustRepositoryConnectionInput,
    expected_kind: &str,
) -> Result<RepositoryTrustChallenge, String> {
    let pending = pending_trust_challenges()
        .lock()
        .remove(&input.confirmation_nonce)
        .ok_or_else(|| "连接确认已失效，请重新测试连接".to_string())?;
    if pending.created_at.elapsed() > Duration::from_secs(5 * 60) {
        return Err("连接确认已过期，请重新测试连接".into());
    }
    let challenge = pending.challenge;
    if challenge.kind != expected_kind
        || challenge.host != input.host
        || challenge.port != input.port
        || challenge.fingerprint_sha256 != input.fingerprint_sha256
    {
        return Err("连接确认与刚才检测到的指纹不匹配".into());
    }
    Ok(challenge)
}

pub fn trust_ssh_host_key(
    conn: &Connection,
    input: TrustRepositoryConnectionInput,
) -> Result<(), String> {
    ensure_repository_tables(conn)?;
    let challenge = consume_pending_trust(&input, "ssh-host-key")?;
    let key_type = challenge
        .key_type
        .as_deref()
        .ok_or_else(|| "SSH 主机密钥缺少类型".to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_repository_ssh_host_keys (
           host, port, key_type, fingerprint_sha256, trusted_at, last_seen_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(host, port, key_type, fingerprint_sha256) DO UPDATE SET
           last_seen_at = excluded.last_seen_at",
        params![
            challenge.host,
            challenge.port as i64,
            key_type,
            challenge.fingerprint_sha256,
            now,
        ],
    )
    .map_err(|error| format!("保存 SSH 主机密钥失败: {error}"))?;
    Ok(())
}

pub fn save_credential(
    conn: &Connection,
    input: SaveCredentialInput,
) -> Result<RepositoryCredentialProfile, String> {
    ensure_repository_tables(conn)?;
    let remote = parse_remote(&input.scope_url)?;
    let auth_kind = input.auth_kind.trim();
    if !matches!(auth_kind, "http-token" | "ssh-agent" | "ssh-key") {
        return Err("不支持的凭证类型".into());
    }
    if auth_kind == "http-token" && !matches!(remote.transport.as_str(), "http" | "https") {
        return Err("Token/密码凭证只能用于 HTTP(S) 仓库".into());
    }
    if matches!(auth_kind, "ssh-agent" | "ssh-key") && remote.transport != "ssh" {
        return Err("SSH 凭证只能用于 SSH 仓库".into());
    }
    let display_name = input.display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 128 {
        return Err("凭证名称必须为 1 到 128 个字符".into());
    }
    let username = input
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(remote.username.clone());
    if username.is_none() {
        return Err("请输入凭证用户名".into());
    }
    let key_path = input
        .ssh_private_key_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if auth_kind == "ssh-key" && key_path.is_none() {
        return Err("请选择 SSH 私钥文件".into());
    }
    if let Some(path) = key_path.as_deref() {
        if !Path::new(path).is_file() {
            return Err("SSH 私钥文件不存在".into());
        }
    }
    let id = input.id.unwrap_or_else(|| generate_id("credential"));
    let secret = input
        .secret
        .filter(|value| !value.is_empty())
        .map(Zeroizing::new);
    let secret_storage =
        if auth_kind == "ssh-agent" || (auth_kind == "ssh-key" && secret.is_none()) {
            session_secrets().lock().remove(&id);
            "none"
        } else if let Some(value) = secret {
            session_secrets().lock().insert(id.clone(), value);
            "session"
        } else if session_secrets().lock().contains_key(&id) {
            "session"
        } else {
            return Err("请输入 Token、密码或私钥口令".into());
        };
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_repository_credentials (
           id, display_name, scope_scheme, scope_host, scope_port, auth_kind,
           username, ssh_private_key_path, secret_storage, secret_locator, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10)
         ON CONFLICT(id) DO UPDATE SET
           display_name = excluded.display_name,
           scope_scheme = excluded.scope_scheme,
           scope_host = excluded.scope_host,
           scope_port = excluded.scope_port,
           auth_kind = excluded.auth_kind,
           username = excluded.username,
           ssh_private_key_path = excluded.ssh_private_key_path,
           secret_storage = excluded.secret_storage,
           secret_locator = NULL,
           updated_at = excluded.updated_at",
        params![
            id,
            display_name,
            remote.transport,
            remote.host,
            remote.port,
            auth_kind,
            username,
            key_path,
            secret_storage,
            now,
        ],
    )
    .map_err(|error| format!("保存凭证元数据失败: {error}"))?;
    get_credential_profile(conn, &id)?.ok_or_else(|| "保存凭证失败".into())
}

pub fn list_credentials(conn: &Connection) -> Result<Vec<RepositoryCredentialProfile>, String> {
    ensure_repository_tables(conn)?;
    let mut statement = conn
        .prepare(
            "SELECT c.id, c.display_name, c.scope_scheme, c.scope_host, c.scope_port,
                    c.auth_kind, c.username, c.ssh_private_key_path, c.secret_storage,
                    COUNT(r.id)
             FROM plugin_repository_credentials c
             LEFT JOIN plugin_repositories r ON r.credential_id = c.id
             GROUP BY c.id ORDER BY c.display_name COLLATE NOCASE, c.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let storage: String = row.get(8)?;
            let available = storage == "none" || session_secrets().lock().contains_key(&id);
            Ok(RepositoryCredentialProfile {
                id,
                display_name: row.get(1)?,
                scope_origin: format!(
                    "{}://{}:{}",
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?
                ),
                auth_kind: row.get(5)?,
                username: row.get(6)?,
                ssh_private_key_path: row.get(7)?,
                secret_storage: storage,
                available,
                referenced_repository_count: row.get::<_, i64>(9)? as usize,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn get_credential_profile(
    conn: &Connection,
    id: &str,
) -> Result<Option<RepositoryCredentialProfile>, String> {
    Ok(list_credentials(conn)?
        .into_iter()
        .find(|profile| profile.id == id))
}

pub fn delete_credential(conn: &Connection, id: &str) -> Result<(), String> {
    ensure_repository_tables(conn)?;
    session_secrets().lock().remove(id);
    conn.execute(
        "DELETE FROM plugin_repository_credentials WHERE id = ?1",
        [id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_credential_record(conn: &Connection, id: &str) -> Result<CredentialRecord, String> {
    conn.query_row(
        "SELECT id, scope_scheme, scope_host, scope_port, auth_kind, username,
                ssh_private_key_path, secret_storage
         FROM plugin_repository_credentials WHERE id = ?1",
        [id],
        |row| {
            Ok(CredentialRecord {
                id: row.get(0)?,
                scope_scheme: row.get(1)?,
                scope_host: row.get(2)?,
                scope_port: row.get::<_, i64>(3)? as u16,
                auth_kind: row.get(4)?,
                username: row.get(5)?,
                ssh_private_key_path: row.get(6)?,
                secret_storage: row.get(7)?,
            })
        },
    )
    .map_err(|_| "找不到仓库凭证".into())
}

fn credential_secret(record: &CredentialRecord) -> Result<Option<Zeroizing<String>>, String> {
    match record.secret_storage.as_str() {
        "none" => Ok(None),
        "session" | "keyring" => session_secrets()
            .lock()
            .get(&record.id)
            .map(|secret| Zeroizing::new(secret.to_string()))
            .ok_or_else(|| "会话凭证已失效，请重新输入".into())
            .map(Some),
        _ => Err("无效的凭证存储类型".into()),
    }
}

fn resolve_credential(
    conn: &Connection,
    config: &RepositoryConfig,
) -> Result<Option<ResolvedCredential>, String> {
    if config.authentication_mode == "anonymous" {
        return Ok(None);
    }
    let credential_id = config
        .credential_id
        .as_deref()
        .ok_or_else(|| "该仓库需要凭证，请重新选择".to_string())?;
    let record = load_credential_record(conn, credential_id)?;
    let remote = parse_remote(&config.url)?;
    if record.scope_scheme != remote.transport
        || record.scope_host != remote.host
        || record.scope_port != remote.port
    {
        return Err("凭证作用域与仓库地址不匹配".into());
    }
    let username = record
        .username
        .clone()
        .or(remote.username)
        .unwrap_or_else(|| "git".into());
    match record.auth_kind.as_str() {
        "http-token" => Ok(Some(ResolvedCredential::Http {
            username,
            secret: credential_secret(&record)?
                .ok_or_else(|| "凭证缺少 Token 或密码".to_string())?,
        })),
        "ssh-agent" => Ok(Some(ResolvedCredential::SshAgent { username })),
        "ssh-key" => {
            let passphrase = credential_secret(&record)?;
            let private_key = PathBuf::from(
                record
                    .ssh_private_key_path
                    .as_deref()
                    .ok_or_else(|| "凭证缺少 SSH 私钥路径".to_string())?,
            );
            Ok(Some(ResolvedCredential::SshKey {
                username,
                private_key,
                passphrase,
            }))
        }
        _ => Err("不支持的凭证类型".into()),
    }
}

pub fn add_repository(
    conn: &Connection,
    input: AddRepositoryInput,
) -> Result<PluginRepository, String> {
    ensure_repository_tables(conn)?;
    let remote = parse_remote(&input.url)?;
    if remote.transport == "http" && !input.allow_insecure_transport {
        return Err("HTTP 仓库需要确认不安全传输".into());
    }
    let authentication_mode =
        input
            .authentication_mode
            .as_deref()
            .unwrap_or(if input.credential_id.is_some() {
                "credential"
            } else {
                "anonymous"
            });
    if !matches!(authentication_mode, "anonymous" | "credential") {
        return Err("无效的认证模式".into());
    }
    if remote.transport == "ssh" && authentication_mode == "anonymous" {
        return Err("SSH 仓库需要选择 SSH Agent 或私钥凭证".into());
    }
    if authentication_mode == "credential" {
        let credential_id = input
            .credential_id
            .as_deref()
            .ok_or_else(|| "请选择仓库凭证".to_string())?;
        let record = load_credential_record(conn, credential_id)?;
        if record.scope_scheme != remote.transport
            || record.scope_host != remote.host
            || record.scope_port != remote.port
        {
            return Err("凭证作用域与仓库地址不匹配".into());
        }
        if remote.transport == "http" && !input.allow_insecure_credentials {
            return Err("通过 HTTP 发送凭证需要单独确认".into());
        }
    }
    let id = generate_id("repository");
    let git_ref = input.git_ref.trim();
    let git_ref = if git_ref.is_empty() { "HEAD" } else { git_ref };
    let index_path = validate_index_path(&input.index_path)?;
    let display_name = validate_display_name(input.display_name.as_deref())?;
    let priority: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(priority), -1) + 1 FROM plugin_repositories",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_repositories (
           id, url, transport, git_ref, index_path, authentication_mode, credential_id,
           allow_insecure_transport, allow_insecure_credentials, display_name,
           enabled, priority, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?12)",
        params![
            id,
            remote.normalized_url,
            remote.transport,
            git_ref,
            index_path,
            authentication_mode,
            input.credential_id,
            input.allow_insecure_transport as i64,
            input.allow_insecure_credentials as i64,
            display_name,
            priority,
            now,
        ],
    )
    .map_err(|error| format!("添加仓库失败: {error}"))?;
    get_repository(conn, &id)?.ok_or_else(|| "添加仓库失败".into())
}

pub fn update_repository(
    app: &AppHandle,
    conn: &Connection,
    input: UpdateRepositoryInput,
) -> Result<PluginRepository, String> {
    ensure_repository_tables(conn)?;
    if has_active_repository_operation(&input.repository_id) {
        return Err("仓库任务正在运行，请等待任务结束后再修改".into());
    }
    let previous = load_repository_config(conn, &input.repository_id)?;
    let remote = parse_remote(&input.url)?;
    if remote.transport == "http" && !input.allow_insecure_transport {
        return Err("HTTP 仓库需要确认不安全传输".into());
    }
    let authentication_mode =
        input
            .authentication_mode
            .as_deref()
            .unwrap_or(if input.credential_id.is_some() {
                "credential"
            } else {
                "anonymous"
            });
    if !matches!(authentication_mode, "anonymous" | "credential") {
        return Err("无效的认证模式".into());
    }
    if remote.transport == "ssh" && authentication_mode == "anonymous" {
        return Err("SSH 仓库需要选择 SSH Agent 或私钥凭证".into());
    }
    if authentication_mode == "credential" {
        let credential_id = input
            .credential_id
            .as_deref()
            .ok_or_else(|| "请选择仓库凭证".to_string())?;
        let record = load_credential_record(conn, credential_id)?;
        if record.scope_scheme != remote.transport
            || record.scope_host != remote.host
            || record.scope_port != remote.port
        {
            return Err("凭证作用域与仓库地址不匹配".into());
        }
        if remote.transport == "http" && !input.allow_insecure_credentials {
            return Err("通过 HTTP 发送凭证需要单独确认".into());
        }
    }
    let git_ref = input.git_ref.trim();
    let git_ref = if git_ref.is_empty() { "HEAD" } else { git_ref };
    let index_path = validate_index_path(&input.index_path)?;
    let display_name = validate_display_name(input.display_name.as_deref())?;
    let connection_changed = previous.url != remote.normalized_url
        || previous.git_ref != git_ref
        || previous.index_path != index_path;
    let authentication_changed = previous.authentication_mode != authentication_mode
        || previous.credential_id != input.credential_id
        || previous.allow_insecure_transport != input.allow_insecure_transport
        || previous.allow_insecure_credentials != input.allow_insecure_credentials;
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE plugin_repositories SET
               url = ?1, transport = ?2, git_ref = ?3, index_path = ?4,
               authentication_mode = ?5, credential_id = ?6,
               allow_insecure_transport = ?7, allow_insecure_credentials = ?8,
               display_name = ?9,
               connection_verified_at = CASE WHEN ?10 OR ?11 THEN NULL ELSE connection_verified_at END,
               updated_at = ?12
             WHERE id = ?13",
            params![
                remote.normalized_url,
                remote.transport,
                git_ref,
                index_path,
                authentication_mode,
                if authentication_mode == "credential" {
                    input.credential_id.as_deref()
                } else {
                    None
                },
                input.allow_insecure_transport as i64,
                input.allow_insecure_credentials as i64,
                display_name,
                connection_changed as i64,
                authentication_changed as i64,
                chrono::Utc::now().to_rfc3339(),
                input.repository_id,
            ],
        )
        .map_err(|error| format!("更新仓库失败: {error}"))?;
    if connection_changed {
        transaction
            .execute(
                "DELETE FROM plugin_repository_plugins WHERE repository_id = ?1",
                [&input.repository_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM plugin_repository_issues WHERE repository_id = ?1",
                [&input.repository_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE plugin_repositories SET
                   remote_repository_id = NULL, remote_name = NULL,
                   remote_description = NULL, remote_homepage = NULL,
                   snapshot_commit = NULL, valid_plugin_count = 0, issue_count = 0,
                   last_success_at = NULL, last_error = NULL
                 WHERE id = ?1",
                [&input.repository_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    if connection_changed {
        if let Ok(path) = repository_cache_dir(app, &input.repository_id) {
            let _ = fs::remove_dir_all(path);
        }
    }
    get_repository(conn, &input.repository_id)?.ok_or_else(|| "更新仓库失败".into())
}

pub fn reorder_repositories(conn: &Connection, repository_ids: &[String]) -> Result<(), String> {
    ensure_repository_tables(conn)?;
    let current: Vec<String> = {
        let mut statement = conn
            .prepare("SELECT id FROM plugin_repositories ORDER BY priority, id")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<_, _>>()?
    };
    let requested: HashSet<_> = repository_ids.iter().collect();
    let existing: HashSet<_> = current.iter().collect();
    if repository_ids.len() != current.len()
        || requested.len() != repository_ids.len()
        || requested != existing
    {
        return Err("仓库排序必须包含当前全部仓库且不能重复".into());
    }
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    for (priority, repository_id) in repository_ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE plugin_repositories SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                params![priority as i64, now, repository_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn list_repositories(conn: &Connection) -> Result<Vec<PluginRepository>, String> {
    ensure_repository_tables(conn)?;
    let profiles: HashMap<_, _> = list_credentials(conn)?
        .into_iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect();
    let mut statement = conn
        .prepare(
            "SELECT id, url, transport, git_ref, index_path, authentication_mode, credential_id,
                    allow_insecure_transport, allow_insecure_credentials, display_name,
                    remote_name, enabled, priority, snapshot_commit, valid_plugin_count,
                    issue_count, connection_verified_at, last_sync_at, last_success_at, last_error
             FROM plugin_repositories ORDER BY priority, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let url: String = row.get(1)?;
            let auth_mode: String = row.get(5)?;
            let credential_id: Option<String> = row.get(6)?;
            let display_name: Option<String> = row.get(9)?;
            let remote_name: Option<String> = row.get(10)?;
            let snapshot_commit: Option<String> = row.get(13)?;
            let fallback_name = parse_remote(&url)
                .map(|remote| remote.host)
                .unwrap_or_else(|_| url.clone());
            let credential_status = if auth_mode == "anonymous" {
                "anonymous"
            } else {
                match credential_id.as_ref().and_then(|value| profiles.get(value)) {
                    Some(profile) if profile.available => "ready",
                    Some(profile)
                        if profile.secret_storage == "session"
                            || profile.secret_storage == "keyring" =>
                    {
                        "session-missing"
                    }
                    Some(_) => "failed",
                    None => "missing",
                }
            };
            Ok(PluginRepository {
                id,
                url,
                transport: row.get(2)?,
                git_ref: row.get(3)?,
                index_path: row.get(4)?,
                authentication_mode: auth_mode,
                credential_id,
                credential_status: credential_status.into(),
                allow_insecure_transport: row.get::<_, i64>(7)? != 0,
                allow_insecure_credentials: row.get::<_, i64>(8)? != 0,
                display_name: display_name.clone(),
                name: display_name.or(remote_name).unwrap_or(fallback_name),
                enabled: row.get::<_, i64>(11)? != 0,
                priority: row.get(12)?,
                snapshot_commit: snapshot_commit.clone(),
                valid_plugin_count: row.get::<_, i64>(14)? as usize,
                issue_count: row.get::<_, i64>(15)? as usize,
                connection_verified_at: row.get(16)?,
                last_sync_at: row.get(17)?,
                last_success_at: row.get(18)?,
                last_error: row.get(19)?,
                has_cached_catalog: snapshot_commit.is_some(),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn get_repository(conn: &Connection, id: &str) -> Result<Option<PluginRepository>, String> {
    Ok(list_repositories(conn)?
        .into_iter()
        .find(|repository| repository.id == id))
}

pub fn set_repository_enabled(conn: &Connection, id: &str, enabled: bool) -> Result<(), String> {
    ensure_repository_tables(conn)?;
    let changed = conn
        .execute(
            "UPDATE plugin_repositories SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![enabled as i64, chrono::Utc::now().to_rfc3339(), id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("找不到插件仓库".into());
    }
    Ok(())
}

pub fn remove_repository(app: &AppHandle, conn: &Connection, id: &str) -> Result<(), String> {
    ensure_repository_tables(conn)?;
    if has_active_repository_operation(id) {
        return Err("仓库任务正在运行，请等待任务结束后再删除".into());
    }
    let changed = conn
        .execute("DELETE FROM plugin_repositories WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("找不到插件仓库".into());
    }
    if let Ok(path) = repository_cache_dir(app, id) {
        let _ = fs::remove_dir_all(path);
    }
    Ok(())
}

fn load_repository_config(conn: &Connection, id: &str) -> Result<RepositoryConfig, String> {
    ensure_repository_tables(conn)?;
    conn.query_row(
        "SELECT id, url, transport, git_ref, index_path, authentication_mode, credential_id,
                allow_insecure_transport, allow_insecure_credentials, proxy_mode, enabled
         FROM plugin_repositories WHERE id = ?1",
        [id],
        |row| {
            Ok(RepositoryConfig {
                id: row.get(0)?,
                url: row.get(1)?,
                transport: row.get(2)?,
                git_ref: row.get(3)?,
                index_path: row.get(4)?,
                authentication_mode: row.get(5)?,
                credential_id: row.get(6)?,
                allow_insecure_transport: row.get::<_, i64>(7)? != 0,
                allow_insecure_credentials: row.get::<_, i64>(8)? != 0,
                proxy_mode: row.get(9)?,
                enabled: row.get::<_, i64>(10)? != 0,
            })
        },
    )
    .map_err(|_| "找不到插件仓库".into())
}

fn set_operation(
    app: &AppHandle,
    operation_id: &str,
    update: impl FnOnce(&mut RepositoryOperation),
) {
    let next = {
        let mut all = operations().lock();
        let Some(operation) = all.get_mut(operation_id) else {
            return;
        };
        update(operation);
        operation.clone()
    };
    let _ = app.emit(OPERATION_EVENT, &next);
}

fn create_operation(kind: &str, repository_id: &str, plugin_id: Option<&str>) -> OperationStarted {
    let mut all = operations().lock();
    if let Some(existing) = all.values().find(|operation| {
        let same_target = if kind == "install" {
            operation.plugin_id.as_deref() == plugin_id
        } else {
            operation.repository_id == repository_id
        };
        operation.kind == kind
            && same_target
            && matches!(operation.status.as_str(), "queued" | "running")
    }) {
        return OperationStarted {
            operation_id: existing.operation_id.clone(),
            reused_existing: true,
        };
    }
    if all.len() >= 100 {
        let mut finished: Vec<_> = all
            .values()
            .filter(|operation| !matches!(operation.status.as_str(), "queued" | "running"))
            .map(|operation| operation.operation_id.clone())
            .collect();
        finished.sort();
        for operation_id in finished.into_iter().take(all.len().saturating_sub(99)) {
            all.remove(&operation_id);
        }
    }
    let operation_id = generate_id("operation");
    all.insert(
        operation_id.clone(),
        RepositoryOperation {
            operation_id: operation_id.clone(),
            kind: kind.into(),
            repository_id: repository_id.into(),
            plugin_id: plugin_id.map(str::to_string),
            status: "queued".into(),
            phase: "queued".into(),
            message: "等待处理".into(),
            transferred_bytes: 0,
            total_items: None,
            completed_items: 0,
            error: None,
        },
    );
    OperationStarted {
        operation_id,
        reused_existing: false,
    }
}

pub fn list_operations() -> Vec<RepositoryOperation> {
    let mut values: Vec<_> = operations().lock().values().cloned().collect();
    values.sort_by(|left, right| right.operation_id.cmp(&left.operation_id));
    values
}

pub fn test_repository_connection(
    app: &AppHandle,
    repository_id: &str,
) -> Result<RepositoryConnectionTest, String> {
    let (config, credential, trust) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock();
        let config = load_repository_config(&conn, repository_id)?;
        if !config.enabled {
            return Ok(RepositoryConnectionTest {
                repository_id: repository_id.to_string(),
                status: "failed".into(),
                message: "插件仓库已停用".into(),
                challenge: None,
            });
        }
        if config.transport == "http" && !config.allow_insecure_transport {
            return Ok(RepositoryConnectionTest {
                repository_id: repository_id.to_string(),
                status: "failed".into(),
                message: "HTTP 仓库尚未确认不安全传输".into(),
                challenge: None,
            });
        }
        if config.transport == "http"
            && config.authentication_mode == "credential"
            && !config.allow_insecure_credentials
        {
            return Ok(RepositoryConnectionTest {
                repository_id: repository_id.to_string(),
                status: "failed".into(),
                message: "HTTP 仓库尚未确认明文发送凭证".into(),
                challenge: None,
            });
        }
        let credential = match resolve_credential(&conn, &config) {
            Ok(value) => value,
            Err(error) => {
                return Ok(RepositoryConnectionTest {
                    repository_id: repository_id.to_string(),
                    status: "failed".into(),
                    message: error,
                    challenge: None,
                });
            }
        };
        let remote = parse_remote(&config.url)?;
        let trust = load_transport_trust(&conn, &remote)?;
        (config, credential, trust)
    };
    match fetch_repository(app, &config, credential, trust, "connection-test") {
        Ok((_repository, commit)) => {
            let state = app.state::<AppState>();
            let conn = state.db.lock();
            conn.execute(
                "UPDATE plugin_repositories SET connection_verified_at = ?1, updated_at = ?1
                 WHERE id = ?2",
                params![chrono::Utc::now().to_rfc3339(), repository_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(RepositoryConnectionTest {
                repository_id: repository_id.to_string(),
                status: "ok".into(),
                message: format!("连接成功，目标 commit {}", &commit[..commit.len().min(8)]),
                challenge: None,
            })
        }
        Err(FetchRepositoryError::TrustRequired(challenge)) => Ok(RepositoryConnectionTest {
            repository_id: repository_id.to_string(),
            status: "trust-required".into(),
            message: "需要确认 SSH 主机密钥".into(),
            challenge: Some(challenge),
        }),
        Err(FetchRepositoryError::Message(message)) => Ok(RepositoryConnectionTest {
            repository_id: repository_id.to_string(),
            status: "failed".into(),
            message,
            challenge: None,
        }),
    }
}

pub fn start_sync(app: AppHandle, repository_id: String) -> OperationStarted {
    let started = create_operation("sync", &repository_id, None);
    if started.reused_existing {
        return started;
    }
    let operation_id = started.operation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        set_operation(&app, &operation_id, |operation| {
            operation.status = "running".into();
            operation.phase = "fetching".into();
            operation.message = "正在拉取仓库".into();
        });
        match sync_repository(&app, &repository_id, &operation_id) {
            Ok(message) => set_operation(&app, &operation_id, |operation| {
                operation.status = "completed".into();
                operation.phase = "completed".into();
                operation.message = message;
                operation.error = None;
            }),
            Err(error) => {
                record_sync_failure(&app, &repository_id, &error);
                set_operation(&app, &operation_id, |operation| {
                    operation.status = "failed".into();
                    operation.phase = "failed".into();
                    operation.message = "仓库同步失败".into();
                    operation.error = Some(error);
                });
            }
        }
    });
    started
}

pub fn start_install(
    app: AppHandle,
    repository_id: String,
    plugin_id: String,
    expected_commit: Option<String>,
) -> OperationStarted {
    let started = create_operation("install", &repository_id, Some(&plugin_id));
    if started.reused_existing {
        return started;
    }
    let operation_id = started.operation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        set_operation(&app, &operation_id, |operation| {
            operation.status = "running".into();
            operation.phase = "materializing-package".into();
            operation.message = "正在准备插件包".into();
        });
        let result =
            install_repository_plugin(&app, &repository_id, &plugin_id, expected_commit.as_deref());
        set_operation(&app, &operation_id, |operation| match result {
            Ok(message) => {
                operation.status = "completed".into();
                operation.phase = "completed".into();
                operation.message = message;
                operation.error = None;
            }
            Err(error) => {
                operation.status = "failed".into();
                operation.phase = "failed".into();
                operation.message = "插件安装失败".into();
                operation.error = Some(error);
            }
        });
    });
    started
}

fn fetch_repository(
    app: &AppHandle,
    config: &RepositoryConfig,
    credential: Option<ResolvedCredential>,
    trust: TransportTrust,
    operation_id: &str,
) -> Result<(Repository, String), FetchRepositoryError> {
    ensure_git_network_options()?;
    let cache_root = repository_cache_dir(app, &config.id)?;
    ensure_dir(&cache_root)?;
    let git_dir = cache_root.join("git");
    let repository = if git_dir.exists() {
        Repository::open_bare(&git_dir).map_err(|error| format!("打开 Git 缓存失败: {error}"))?
    } else {
        Repository::init_bare(&git_dir).map_err(|error| format!("创建 Git 缓存失败: {error}"))?
    };

    let mut callbacks = RemoteCallbacks::new();
    if let Some(auth) = credential {
        let expected_remote = parse_remote(&config.url)?;
        callbacks.credentials(move |url, username_from_url, allowed| {
            let callback_remote =
                parse_remote(url).map_err(|error| git2::Error::from_str(&error))?;
            if callback_remote.transport != expected_remote.transport
                || callback_remote.host != expected_remote.host
                || callback_remote.port != expected_remote.port
            {
                return Err(git2::Error::from_str(
                    "拒绝向不同协议、主机或端口的重定向地址发送仓库凭证",
                ));
            }
            let fallback_user = username_from_url.unwrap_or("git");
            match &auth {
                ResolvedCredential::Http { username, secret }
                    if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) =>
                {
                    Cred::userpass_plaintext(username, secret)
                }
                ResolvedCredential::SshAgent { username }
                    if allowed.contains(CredentialType::SSH_KEY) =>
                {
                    Cred::ssh_key_from_agent(if username.is_empty() {
                        fallback_user
                    } else {
                        username
                    })
                }
                ResolvedCredential::SshKey {
                    username,
                    private_key,
                    passphrase,
                } if allowed.contains(CredentialType::SSH_KEY) => Cred::ssh_key(
                    if username.is_empty() {
                        fallback_user
                    } else {
                        username
                    },
                    None,
                    private_key,
                    passphrase.as_deref().map(String::as_str),
                ),
                ResolvedCredential::SshAgent { username }
                | ResolvedCredential::SshKey { username, .. }
                    if allowed.contains(CredentialType::USERNAME) =>
                {
                    Cred::username(if username.is_empty() {
                        fallback_user
                    } else {
                        username
                    })
                }
                _ => Err(git2::Error::from_str("远程仓库不接受已配置的凭证类型")),
            }
        });
    }
    let certificate_remote = parse_remote(&config.url)?;
    let certificate_outcome = Arc::new(Mutex::new(None::<CertificateOutcome>));
    let callback_outcome = certificate_outcome.clone();
    callbacks.certificate_check(move |certificate, hostname| {
        if certificate.as_x509().is_some() {
            return Ok(CertificateCheckStatus::CertificatePassthrough);
        }
        if !hostname.eq_ignore_ascii_case(&certificate_remote.host) {
            *callback_outcome.lock() = Some(CertificateOutcome::Changed(format!(
                "远程连接跳转到了其他主机 {hostname}，已阻止主机密钥确认"
            )));
            return Err(git2::Error::from_str(
                "certificate callback host does not match repository host",
            ));
        }
        if let Some(host_key) = certificate.as_hostkey() {
            let fingerprint = host_key
                .hash_sha256()
                .map(|hash| {
                    format!(
                        "SHA256:{}",
                        base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash)
                    )
                })
                .or_else(|| host_key.hostkey().map(fingerprint_sha256));
            let Some(fingerprint) = fingerprint else {
                *callback_outcome.lock() = Some(CertificateOutcome::Changed(
                    "无法计算 SSH 主机密钥指纹，已阻止连接".into(),
                ));
                return Err(git2::Error::from_str("SSH host key has no fingerprint"));
            };
            let key_type = host_key
                .hostkey_type()
                .map(|value| value.name())
                .unwrap_or("unknown");
            if trust
                .ssh_host_keys
                .contains(&(key_type.to_string(), fingerprint.clone()))
            {
                return Ok(CertificateCheckStatus::CertificateOk);
            }
            let challenge = ssh_challenge(&certificate_remote, key_type, fingerprint);
            let outcome = if trust.ssh_host_keys.is_empty() {
                CertificateOutcome::TrustRequired(challenge)
            } else {
                CertificateOutcome::Changed(format!(
                    "SSH 主机密钥已变化，已阻止连接。当前指纹：{}",
                    challenge.fingerprint_sha256
                ))
            };
            *callback_outcome.lock() = Some(outcome);
            return Err(git2::Error::from_str("SSH host key is not trusted"));
        }
        Ok(CertificateCheckStatus::CertificatePassthrough)
    });
    let app_progress = app.clone();
    let operation_progress = operation_id.to_string();
    let abort_reason = Arc::new(Mutex::new(None::<String>));
    let transfer_abort_reason = abort_reason.clone();
    let fetch_started = Instant::now();
    callbacks.transfer_progress(move |progress| {
        if progress.received_bytes() > MAX_GIT_TRANSFER_BYTES {
            *transfer_abort_reason.lock() = Some("Git 拉取超过 1 GiB 限制".into());
            return false;
        }
        if fetch_started.elapsed() > GIT_FETCH_TIMEOUT {
            *transfer_abort_reason.lock() = Some("Git 拉取超过 5 分钟限制".into());
            return false;
        }
        set_operation(&app_progress, &operation_progress, |operation| {
            operation.transferred_bytes = progress.received_bytes() as u64;
            operation.total_items = Some(progress.total_objects());
            operation.completed_items = progress.received_objects();
        });
        true
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options.depth(1);
    let mut proxy = ProxyOptions::new();
    if config.proxy_mode != "none" {
        proxy.auto();
        fetch_options.proxy_options(proxy);
    }
    let mut remote = repository
        .remote_anonymous(&config.url)
        .map_err(|error| format!("创建 Git remote 失败: {error}"))?;
    if let Err(error) = remote.fetch(&[config.git_ref.as_str()], Some(&mut fetch_options), None) {
        if let Some(reason) = abort_reason.lock().clone() {
            return Err(FetchRepositoryError::Message(reason));
        }
        if let Some(outcome) = certificate_outcome.lock().take() {
            return match outcome {
                CertificateOutcome::TrustRequired(challenge) => {
                    remember_pending_trust(challenge.clone());
                    Err(FetchRepositoryError::TrustRequired(challenge))
                }
                CertificateOutcome::Changed(message) => Err(FetchRepositoryError::Message(message)),
            };
        }
        return Err(FetchRepositoryError::Message(format!(
            "Git 拉取失败: {error}"
        )));
    }
    let commit = repository
        .revparse_single("FETCH_HEAD")
        .and_then(|object| object.peel_to_commit())
        .map_err(|error| format!("解析远程 commit 失败: {error}"))?;
    let commit_id = commit.id().to_string();
    drop(commit);
    drop(remote);
    Ok((repository, commit_id))
}

fn sync_repository(
    app: &AppHandle,
    repository_id: &str,
    operation_id: &str,
) -> Result<String, String> {
    let (config, credential, trust, previous) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock();
        let config = load_repository_config(&conn, repository_id)?;
        if !config.enabled {
            return Err("插件仓库已停用".into());
        }
        if config.transport == "http" && !config.allow_insecure_transport {
            return Err("HTTP 仓库尚未确认不安全传输".into());
        }
        if config.transport == "http"
            && config.authentication_mode == "credential"
            && !config.allow_insecure_credentials
        {
            return Err("HTTP 仓库尚未确认明文发送凭证".into());
        }
        let credential = resolve_credential(&conn, &config)?;
        let remote = parse_remote(&config.url)?;
        let trust = load_transport_trust(&conn, &remote)?;
        let mut previous = HashMap::new();
        let mut statement = conn
            .prepare(
                "SELECT plugin_id, version, package_hash FROM plugin_repository_plugins
                 WHERE repository_id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([repository_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (id, version, hash) = row.map_err(|error| error.to_string())?;
            previous.insert(id, (version, hash));
        }
        (config, credential, trust, previous)
    };

    let fetch_result = fetch_repository(app, &config, credential, trust, operation_id);
    let (repository, commit_id) = match fetch_result {
        Ok(value) => value,
        Err(error) => {
            let error = error.to_string();
            record_sync_failure(app, repository_id, &error);
            return Err(error);
        }
    };
    set_operation(app, operation_id, |operation| {
        operation.phase = "reading-index".into();
        operation.message = "正在读取仓库索引".into();
    });
    let commit = repository
        .find_commit(git2::Oid::from_str(&commit_id).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let root = commit.tree().map_err(|error| error.to_string())?;
    let index_blob = root
        .get_path(Path::new(&config.index_path))
        .map_err(|_| format!("仓库中找不到索引文件 {}", config.index_path))?
        .to_object(&repository)
        .and_then(|object| object.peel_to_blob())
        .map_err(|error| format!("读取仓库索引失败: {error}"))?;
    if index_blob.content().len() > INDEX_MAX_BYTES {
        return Err("仓库索引超过 2 MiB".into());
    }
    let index = parse_repository_index(index_blob.content())?;
    validate_repository_index(&index)?;

    set_operation(app, operation_id, |operation| {
        operation.phase = "validating-plugins".into();
        operation.message = "正在检查插件构建".into();
        operation.total_items = Some(index.plugins.len());
        operation.completed_items = 0;
    });
    let inspect_root = staging_dir(app)?.join(format!("repo-sync-{}", operation_id));
    let _ = fs::remove_dir_all(&inspect_root);
    ensure_dir(&inspect_root)?;
    let mut catalog = Vec::new();
    let mut issues = Vec::new();
    for (position, entry) in index.plugins.iter().enumerate() {
        let result = inspect_indexed_plugin(
            &repository,
            &root,
            entry,
            &inspect_root.join(position.to_string()),
        );
        match result {
            Ok(record) => {
                let mut issue = None;
                if let Some((old_version, old_hash)) = previous.get(&entry.id) {
                    if old_version == &record.manifest.version && old_hash != &record.package_hash {
                        issue = Some("同一版本的 dist 内容已经变化，请提升插件版本".to_string());
                    } else if let (Ok(old), Ok(next)) = (
                        Version::parse(old_version),
                        Version::parse(&record.manifest.version),
                    ) {
                        if next < old {
                            issue = Some("插件版本低于上一次仓库快照".to_string());
                        }
                    }
                }
                if let Some(error) = issue {
                    issues.push((
                        position.to_string(),
                        Some(entry.id.clone()),
                        Some(entry.path.clone()),
                        error,
                    ));
                } else {
                    catalog.push(record);
                }
            }
            Err(error) => {
                issues.push((
                    position.to_string(),
                    Some(entry.id.clone()),
                    Some(entry.path.clone()),
                    error,
                ));
            }
        }
        set_operation(app, operation_id, |operation| {
            operation.completed_items = position + 1;
        });
    }
    let _ = fs::remove_dir_all(&inspect_root);

    set_operation(app, operation_id, |operation| {
        operation.phase = "saving-catalog".into();
        operation.message = "正在保存插件目录".into();
    });
    {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock();
        ensure_repository_tables(&conn)?;
        let transaction = conn.transaction().map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM plugin_repository_plugins WHERE repository_id = ?1",
                [repository_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM plugin_repository_issues WHERE repository_id = ?1",
                [repository_id],
            )
            .map_err(|error| error.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        for record in &catalog {
            transaction
                .execute(
                    "INSERT INTO plugin_repository_plugins (
                       repository_id, plugin_id, plugin_root, version, package_hash, name,
                       publisher, description, kind, categories_json, platforms_json,
                       engine_tempo, engine_plugin_api, requires_node_runtime,
                       icon_data_url, compatible, incompatible_reason, source_commit, indexed_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                               ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                    params![
                        repository_id,
                        record.plugin_id,
                        record.plugin_root,
                        record.manifest.version,
                        record.package_hash,
                        record.manifest.name,
                        record.manifest.publisher,
                        record.manifest.description,
                        record.manifest.resolved_kind(),
                        serde_json::to_string(&record.manifest.categories)
                            .unwrap_or_else(|_| "[]".into()),
                        serde_json::to_string(&record.manifest.platforms)
                            .unwrap_or_else(|_| "[]".into()),
                        record.manifest.engines.tempo,
                        record.manifest.engines.plugin_api,
                        record.manifest.requires_node_runtime() as i64,
                        record.icon_data_url,
                        record.compatible as i64,
                        record.incompatible_reason,
                        commit_id,
                        now,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        for (key, plugin_id, plugin_root, error) in &issues {
            transaction
                .execute(
                    "INSERT INTO plugin_repository_issues (
                       repository_id, entry_key, declared_plugin_id, plugin_root, error,
                       source_commit, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        repository_id,
                        key,
                        plugin_id,
                        plugin_root,
                        error,
                        commit_id,
                        now
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction
            .execute(
                "UPDATE plugin_repositories SET
                   remote_repository_id = ?1, remote_name = ?2, remote_description = ?3,
                   remote_homepage = ?4, snapshot_commit = ?5, valid_plugin_count = ?6,
                   issue_count = ?7, connection_verified_at = ?8, last_sync_at = ?8,
                   last_success_at = ?8, last_error = NULL, updated_at = ?8
                 WHERE id = ?9",
                params![
                    index.id,
                    index.name.as_deref(),
                    index.description,
                    index.homepage,
                    commit_id,
                    catalog.len() as i64,
                    issues.len() as i64,
                    now,
                    repository_id,
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    Ok(format!("已同步 {} 个插件", catalog.len()))
}

fn record_sync_failure(app: &AppHandle, repository_id: &str, error: &str) {
    let state = app.state::<AppState>();
    let conn = state.db.lock();
    let _ = ensure_repository_tables(&conn);
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        "UPDATE plugin_repositories SET last_sync_at = ?1, last_error = ?2, updated_at = ?1
         WHERE id = ?3",
        params![now, error, repository_id],
    );
}

fn validate_repository_index(index: &RepositoryIndex) -> Result<(), String> {
    if index.schema_version != 1 {
        return Err(format!("不支持仓库索引版本 {}", index.schema_version));
    }
    if let Some(name) = index.name.as_deref() {
        if name.trim().is_empty() || name.chars().count() > 128 {
            return Err("仓库索引 name 无效".into());
        }
    }
    if !is_valid_repository_index_id(&index.id) {
        return Err("仓库索引 id 无效".into());
    }
    if index
        .description
        .as_deref()
        .is_some_and(|description| description.chars().count() > 1024)
    {
        return Err("仓库索引 description 超过 1024 个字符".into());
    }
    if let Some(homepage) = index.homepage.as_deref() {
        if homepage.len() > 2048 {
            return Err("仓库索引 homepage 过长".into());
        }
        let parsed = url::Url::parse(homepage)
            .map_err(|error| format!("仓库索引 homepage 无效: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("仓库索引 homepage 必须使用 HTTP(S)".into());
        }
    }
    if index.plugins.len() > MAX_REPOSITORY_PLUGINS {
        return Err("仓库索引包含过多插件".into());
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let mut normalized_paths: Vec<String> = Vec::new();
    for plugin in &index.plugins {
        if !is_valid_plugin_id(&plugin.id) || !ids.insert(plugin.id.to_ascii_lowercase()) {
            return Err(format!("仓库索引包含无效或重复插件 ID: {}", plugin.id));
        }
        let path = validate_plugin_root(&plugin.path)?;
        let folded = path.to_lowercase();
        if !paths.insert(folded.clone()) {
            return Err(format!("仓库索引包含重复插件路径: {}", plugin.path));
        }
        for previous in &normalized_paths {
            if folded.starts_with(&format!("{previous}/"))
                || previous.starts_with(&format!("{folded}/"))
            {
                return Err("仓库索引中的插件目录不能互相嵌套".into());
            }
        }
        normalized_paths.push(folded);
    }
    Ok(())
}

fn parse_repository_index(bytes: &[u8]) -> Result<RepositoryIndex, String> {
    let strict: StrictJsonValue =
        serde_json::from_slice(bytes).map_err(|error| format!("仓库索引 JSON 无效: {error}"))?;
    serde_json::from_value(strict.0).map_err(|error| format!("仓库索引 JSON 无效: {error}"))
}

fn validate_plugin_root(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git")
        || path.nfc().collect::<String>() != path
    {
        return Err(format!("无效的插件目录: {path}"));
    }
    Ok(path.to_string())
}

fn inspect_indexed_plugin(
    repository: &Repository,
    root: &git2::Tree<'_>,
    entry: &RepositoryIndexPlugin,
    temp: &Path,
) -> Result<CatalogRecord, String> {
    let plugin_root = validate_plugin_root(&entry.path)?;
    let dist_path = format!("{plugin_root}/dist");
    let dist = root
        .get_path(Path::new(&dist_path))
        .map_err(|_| format!("插件目录缺少 dist: {dist_path}"))?
        .to_object(repository)
        .and_then(|object| object.peel_to_tree())
        .map_err(|error| format!("读取 dist 失败: {error}"))?;
    let _ = fs::remove_dir_all(temp);
    ensure_dir(temp)?;
    let materialized = materialize_tree(repository, &dist, temp);
    let result = (|| {
        materialized?;
        let inspected = inspect_package(temp)?;
        if inspected.manifest.id != entry.id {
            return Err(format!(
                "索引 ID {} 与 dist Manifest ID {} 不一致",
                entry.id, inspected.manifest.id
            ));
        }
        let (compatible, incompatible_reason) = check_compatibility(&inspected.manifest)?;
        let icon_data_url = manifest_icon_data_url(temp, &inspected.manifest);
        Ok(CatalogRecord {
            plugin_id: entry.id.clone(),
            plugin_root,
            manifest: inspected.manifest,
            package_hash: inspected.package_hash,
            icon_data_url,
            compatible,
            incompatible_reason,
        })
    })();
    let _ = fs::remove_dir_all(temp);
    result
}

fn materialize_tree(
    repository: &Repository,
    tree: &git2::Tree<'_>,
    dest: &Path,
) -> Result<(), String> {
    let mut file_count = 0usize;
    let mut total_bytes = 0usize;
    let mut seen_paths = HashSet::new();
    fn walk(
        repository: &Repository,
        tree: &git2::Tree<'_>,
        dest: &Path,
        rel: &str,
        file_count: &mut usize,
        total_bytes: &mut usize,
        seen_paths: &mut HashSet<String>,
    ) -> Result<(), String> {
        for entry in tree.iter() {
            let name = entry
                .name()
                .map_err(|_| "Git tree 包含非 UTF-8 路径".to_string())?;
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains(['/', '\\', '\0', ':'])
            {
                return Err(format!("Git tree 包含非法路径: {name}"));
            }
            let next_rel = if rel.is_empty() {
                name.to_string()
            } else {
                format!("{rel}/{name}")
            };
            if next_rel.nfc().collect::<String>() != next_rel {
                return Err(format!("Git tree 路径不是 NFC 格式: {next_rel}"));
            }
            if !seen_paths.insert(next_rel.to_lowercase()) {
                return Err(format!("dist 包含大小写冲突路径: {next_rel}"));
            }
            let target = dest.join(PathBuf::from(
                next_rel.replace('/', std::path::MAIN_SEPARATOR_STR),
            ));
            match entry.kind() {
                Some(ObjectType::Tree) => {
                    ensure_dir(&target)?;
                    let child = entry
                        .to_object(repository)
                        .and_then(|object| object.peel_to_tree())
                        .map_err(|error| format!("读取 Git tree 失败: {error}"))?;
                    walk(
                        repository,
                        &child,
                        dest,
                        &next_rel,
                        file_count,
                        total_bytes,
                        seen_paths,
                    )?;
                }
                Some(ObjectType::Blob) => {
                    if entry.filemode() == 0o120000 {
                        return Err(format!("dist 不允许符号链接: {next_rel}"));
                    }
                    let blob = entry
                        .to_object(repository)
                        .and_then(|object| object.peel_to_blob())
                        .map_err(|error| format!("读取 Git blob 失败: {error}"))?;
                    let bytes = blob.content();
                    if bytes.len() > MAX_GIT_FILE_BYTES {
                        return Err(format!("dist 文件过大: {next_rel}"));
                    }
                    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
                        return Err(format!("dist 包含 Git LFS pointer: {next_rel}"));
                    }
                    *file_count += 1;
                    *total_bytes = total_bytes.saturating_add(bytes.len());
                    if *file_count > MAX_GIT_FILES || *total_bytes > MAX_GIT_PACKAGE_BYTES {
                        return Err("dist 超过文件数或总大小限制".into());
                    }
                    if let Some(parent) = target.parent() {
                        ensure_dir(parent)?;
                    }
                    fs::write(&target, bytes)
                        .map_err(|error| format!("写入 {} 失败: {error}", target.display()))?;
                }
                Some(ObjectType::Commit) => {
                    return Err(format!("dist 不允许 Git submodule: {next_rel}"));
                }
                _ => return Err(format!("dist 包含不支持的 Git 对象: {next_rel}")),
            }
        }
        Ok(())
    }
    walk(
        repository,
        tree,
        dest,
        "",
        &mut file_count,
        &mut total_bytes,
        &mut seen_paths,
    )?;
    if file_count == 0 {
        return Err("dist 目录为空".into());
    }
    Ok(())
}

fn check_compatibility(manifest: &PluginManifest) -> Result<(bool, Option<String>), String> {
    Version::parse(&manifest.version)
        .map_err(|error| format!("插件版本不是合法 SemVer: {error}"))?;
    let tempo_req = VersionReq::parse(&manifest.engines.tempo)
        .map_err(|error| format!("engines.tempo 无效: {error}"))?;
    let api_req = VersionReq::parse(&manifest.engines.plugin_api)
        .map_err(|error| format!("engines.pluginApi 无效: {error}"))?;
    if !manifest.supports_platform(current_host_platform()) {
        return Ok((
            false,
            Some(format!("不支持当前平台 {}", current_host_platform())),
        ));
    }
    let tempo = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    if !tempo_req.matches(&tempo) {
        return Ok((
            false,
            Some(format!("需要 Tempo {}", manifest.engines.tempo)),
        ));
    }
    let api = Version::parse(HOST_API_VERSION).map_err(|error| error.to_string())?;
    if !api_req.matches(&api) {
        return Ok((
            false,
            Some(format!("需要 Plugin API {}", manifest.engines.plugin_api)),
        ));
    }
    Ok((true, None))
}

fn manifest_icon_data_url(root: &Path, manifest: &PluginManifest) -> Option<String> {
    let relative = manifest
        .contributes
        .apps
        .iter()
        .find_map(|app| app.icon.as_deref())
        .or_else(|| {
            manifest
                .contributes
                .actions
                .iter()
                .find_map(|action| action.icon.as_deref())
        })?;
    icons::data_url_from_package_file(root, relative)
}

pub fn list_catalog_plugins(
    conn: &Connection,
    query: Option<&str>,
) -> Result<Vec<RepositoryCatalogPlugin>, String> {
    ensure_repository_tables(conn)?;
    let query = query.unwrap_or("").trim().to_lowercase();
    let mut installed = HashMap::new();
    let mut installed_statement = conn
        .prepare(
            "SELECT p.id, p.current_version, p.pending_version, v.package_hash,
                    v.source_repository_id
             FROM plugins p LEFT JOIN plugin_versions v
               ON v.plugin_id = p.id AND v.version = p.current_version",
        )
        .map_err(|error| error.to_string())?;
    let installed_rows = installed_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in installed_rows {
        let (id, version, pending, hash, source_repository_id) =
            row.map_err(|error| error.to_string())?;
        installed.insert(id, (version, pending, hash, source_repository_id));
    }

    let mut statement = conn
        .prepare(
            "SELECT p.plugin_id, p.name, p.publisher, p.description, p.icon_data_url,
                    p.categories_json, p.version, p.package_hash, p.source_commit,
                    p.compatible, p.incompatible_reason, r.id,
                    COALESCE(r.display_name, r.remote_name, r.url), r.priority
             FROM plugin_repository_plugins p
             JOIN plugin_repositories r ON r.id = p.repository_id
             WHERE r.enabled = 1
             ORDER BY r.priority, p.name COLLATE NOCASE, p.plugin_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? != 0,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, i64>(13)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut rows: Vec<_> = rows
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    rows.sort_by(|left, right| {
        let left_source = installed
            .get(&left.0)
            .and_then(|value| value.3.as_deref())
            .is_some_and(|source| source == left.11);
        let right_source = installed
            .get(&right.0)
            .and_then(|value| value.3.as_deref())
            .is_some_and(|source| source == right.11);
        left.0
            .cmp(&right.0)
            .then_with(|| right_source.cmp(&left_source))
            .then_with(|| left.13.cmp(&right.13))
    });
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for row in rows {
        let (
            id,
            name,
            publisher,
            description,
            icon_url,
            categories_json,
            version,
            package_hash,
            source_commit,
            compatible,
            incompatible_reason,
            repository_id,
            repository_name,
            _priority,
        ) = row;
        if !seen.insert(id.clone()) {
            continue;
        }
        let haystack = format!(
            "{} {} {} {} {}",
            id,
            name,
            publisher.as_deref().unwrap_or(""),
            description.as_deref().unwrap_or(""),
            categories_json,
        )
        .to_lowercase();
        if !query.is_empty() && !haystack.contains(&query) {
            continue;
        }
        let (installed_version, pending_version, installed_hash, installed_source) = installed
            .get(&id)
            .cloned()
            .map(|(version, pending, hash, source)| (Some(version), pending, hash, source))
            .unwrap_or((None, None, None, None));
        let locked_to_other_source = installed_source
            .as_deref()
            .is_some_and(|source| source != repository_id);
        let action = if !compatible {
            "unavailable"
        } else if pending_version.is_some() {
            "pending"
        } else if locked_to_other_source {
            "installed"
        } else if let Some(current) = installed_version.as_deref() {
            match (Version::parse(&version), Version::parse(current)) {
                (Ok(next), Ok(current_version)) if next > current_version => "update",
                (Ok(next), Ok(current_version)) if next == current_version => {
                    if installed_hash.as_deref() == Some(package_hash.as_str()) {
                        "installed"
                    } else {
                        "conflict"
                    }
                }
                _ => "installed",
            }
        } else {
            "install"
        };
        output.push(RepositoryCatalogPlugin {
            id,
            name,
            publisher,
            description,
            icon_url,
            categories: serde_json::from_str(&categories_json).unwrap_or_default(),
            version,
            package_hash,
            source_commit,
            repository_id,
            repository_name,
            compatible,
            incompatible_reason,
            installed_version,
            pending_version,
            action: action.into(),
        });
    }
    output.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(output)
}

pub fn list_repository_issues(
    conn: &Connection,
    repository_id: &str,
) -> Result<Vec<RepositoryIssue>, String> {
    ensure_repository_tables(conn)?;
    let mut statement = conn
        .prepare(
            "SELECT declared_plugin_id, plugin_root, error, source_commit
             FROM plugin_repository_issues WHERE repository_id = ?1 ORDER BY entry_key",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([repository_id], |row| {
            Ok(RepositoryIssue {
                repository_id: repository_id.to_string(),
                plugin_id: row.get(0)?,
                plugin_root: row.get(1)?,
                error: row.get(2)?,
                source_commit: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn install_repository_plugin(
    app: &AppHandle,
    repository_id: &str,
    plugin_id: &str,
    expected_commit: Option<&str>,
) -> Result<String, String> {
    let (config, plugin_root, version, expected_hash, source_commit, compatible) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock();
        let config = load_repository_config(&conn, repository_id)?;
        if !config.enabled {
            return Err("插件仓库已停用".into());
        }
        let row = conn
            .query_row(
                "SELECT plugin_root, version, package_hash, source_commit, compatible
                 FROM plugin_repository_plugins WHERE repository_id = ?1 AND plugin_id = ?2",
                params![repository_id, plugin_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)? != 0,
                    ))
                },
            )
            .map_err(|_| "仓库中找不到该插件".to_string())?;
        let installed: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT current_version, pending_version FROM plugins WHERE id = ?1",
                [plugin_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some((current, pending)) = installed {
            if let (Ok(current), Ok(next)) = (Version::parse(&current), Version::parse(&row.1)) {
                if next < current {
                    return Err("仓库版本低于已安装版本，暂不支持降级".into());
                }
            }
            if pending
                .as_deref()
                .is_some_and(|pending| pending != row.1.as_str())
            {
                return Err("该插件已有待切换版本，请先处理后再安装其他版本".into());
            }
        }
        (config, row.0, row.1, row.2, row.3, row.4)
    };
    if !compatible {
        return Err("该插件与当前 Tempo 或平台不兼容".into());
    }
    if expected_commit.is_some_and(|value| value != source_commit) {
        return Err("仓库目录已经更新，请刷新后重试".into());
    }
    let repository = Repository::open_bare(repository_cache_dir(app, &config.id)?.join("git"))
        .map_err(|error| format!("打开仓库缓存失败: {error}"))?;
    let commit = repository
        .find_commit(git2::Oid::from_str(&source_commit).map_err(|error| error.to_string())?)
        .map_err(|_| "仓库快照已经丢失，请重新同步".to_string())?;
    let root = commit.tree().map_err(|error| error.to_string())?;
    let dist_path = format!("{plugin_root}/dist");
    let dist = root
        .get_path(Path::new(&dist_path))
        .map_err(|_| "仓库快照缺少插件 dist".to_string())?
        .to_object(&repository)
        .and_then(|object| object.peel_to_tree())
        .map_err(|error| error.to_string())?;
    let temp = staging_dir(app)?.join(generate_id("repo-install"));
    let result = (|| {
        ensure_dir(&temp)?;
        materialize_tree(&repository, &dist, &temp)?;
        let inspected = inspect_package(&temp)?;
        if inspected.manifest.id != plugin_id
            || inspected.manifest.version != version
            || inspected.package_hash != expected_hash
        {
            return Err("仓库 dist 与已同步目录不一致".into());
        }
        let installed = import_directory(app, &temp)?;
        let state = app.state::<AppState>();
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        ensure_repository_tables(&conn)?;
        record_installed_version(
            &conn,
            &installed.plugin_id,
            &installed.version,
            &installed.package_hash,
            inspected.manifest.publisher.as_deref(),
            &format!("repository:{repository_id}"),
        )?;
        conn.execute(
            "UPDATE plugin_versions SET source_repository_id = ?1, source_commit = ?2,
                    source_plugin_root = ?3 WHERE plugin_id = ?4 AND version = ?5",
            params![
                repository_id,
                source_commit,
                plugin_root,
                plugin_id,
                version
            ],
        )
        .map_err(|error| format!("记录仓库安装来源失败: {error}"))?;
        Ok(format!("已安装 {}@{}，等待信任", plugin_id, version))
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(generate_id("tempo-repository-test"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_supported_remote_formats() {
        let https = parse_remote("https://gitlab.example.com/team/plugins.git").unwrap();
        assert_eq!(https.transport, "https");
        assert_eq!(https.host, "gitlab.example.com");
        let ssh = parse_remote("git@gitlab.example.com:team/plugins.git").unwrap();
        assert_eq!(ssh.transport, "ssh");
        assert_eq!(ssh.username.as_deref(), Some("git"));
    }

    #[test]
    fn rejects_credentials_in_http_url() {
        assert!(parse_remote("https://token@gitlab.example.com/team/plugins.git").is_err());
        assert!(
            parse_remote("https://gitlab.example.com/team/plugins.git?private_token=x").is_err()
        );
        assert!(parse_remote("ssh://git:password@gitlab.example.com/team/plugins.git").is_err());
    }

    #[test]
    fn validates_repository_index_paths() {
        let valid = RepositoryIndex {
            _json_schema: None,
            schema_version: 1,
            id: "7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d".into(),
            name: None,
            description: None,
            homepage: None,
            plugins: vec![RepositoryIndexPlugin {
                id: "com.example.hello".into(),
                path: "plugins/com.example.hello".into(),
            }],
        };
        assert!(validate_repository_index(&valid).is_ok());
        assert!(validate_plugin_root("plugins\\com.example.hello").is_err());
        assert!(validate_index_path("indexes\\plugins.json").is_err());
    }

    #[test]
    fn parses_repository_index_fixtures() {
        let valid =
            include_bytes!("../../../../docs/schemas/fixtures/plugin-repository/valid-basic.json");
        let index = parse_repository_index(valid).expect("valid fixture");
        validate_repository_index(&index).expect("valid repository index");

        let nested = include_bytes!(
            "../../../../docs/schemas/fixtures/plugin-repository/invalid-nested-paths.json"
        );
        let nested = parse_repository_index(nested).expect("structurally valid fixture");
        assert!(validate_repository_index(&nested).is_err());

        let backslash = include_bytes!(
            "../../../../docs/schemas/fixtures/plugin-repository/invalid-backslash-path.json"
        );
        let backslash = parse_repository_index(backslash).expect("structurally valid fixture");
        assert!(validate_repository_index(&backslash).is_err());

        let with_schema = br#"{
          "$schema": "./schema/plugin-repository.schema.json",
          "schemaVersion": 1,
          "id": "7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d",
          "plugins": []
        }"#;
        let with_schema = parse_repository_index(with_schema).expect("$schema is allowed");
        validate_repository_index(&with_schema).expect("empty plugin list is valid");
    }

    #[test]
    fn rejects_duplicate_json_keys() {
        let raw = br#"{
          "schemaVersion": 1,
          "id": "7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d",
          "name": "First",
          "name": "Second",
          "plugins": []
        }"#;
        assert!(parse_repository_index(raw).is_err());
    }

    #[test]
    fn inspects_dist_from_a_git_commit() {
        let temp = TempDir::new();
        let plugin_root = temp.0.join("plugins/com.example.hello/dist");
        fs::create_dir_all(&plugin_root).expect("create plugin dist");
        fs::write(
            plugin_root.join("manifest.json"),
            r#"{
              "manifestVersion": 2,
              "id": "com.example.hello",
              "name": "Hello",
              "version": "1.0.0",
              "engines": { "tempo": "*", "pluginApi": "*" },
              "contributes": {
                "apps": [{ "id": "main", "name": "Hello", "entry": "index.html" }]
              }
            }"#,
        )
        .expect("write manifest");
        fs::write(
            plugin_root.join("index.html"),
            "<!doctype html><title>Hello</title>",
        )
        .expect("write UI entry");

        let repository = Repository::init(&temp.0).expect("init repository");
        let mut index = repository.index().expect("open index");
        index
            .add_all(["plugins"], git2::IndexAddOption::DEFAULT, None)
            .expect("add plugin files");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("find tree");
        let signature =
            git2::Signature::now("Tempo Test", "tempo@example.com").expect("create signature");
        let commit_id = repository
            .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .expect("commit fixture");
        drop(tree);
        let commit = repository.find_commit(commit_id).expect("find commit");
        let root = commit.tree().expect("read commit tree");
        let entry = RepositoryIndexPlugin {
            id: "com.example.hello".into(),
            path: "plugins/com.example.hello".into(),
        };
        let inspected =
            inspect_indexed_plugin(&repository, &root, &entry, &temp.0.join("inspection"))
                .expect("inspect indexed plugin");
        assert_eq!(inspected.plugin_id, "com.example.hello");
        assert_eq!(inspected.manifest.version, "1.0.0");
        assert!(inspected.compatible);
        assert!(!inspected.package_hash.is_empty());
    }

    #[test]
    fn trust_confirmation_is_nonce_and_origin_bound() {
        let conn = Connection::open_in_memory().expect("open database");
        conn.execute_batch(
            "CREATE TABLE plugin_versions (
               plugin_id TEXT NOT NULL,
               version TEXT NOT NULL,
               PRIMARY KEY(plugin_id, version)
             );",
        )
        .expect("create plugin version table");
        ensure_repository_tables(&conn).expect("create repository tables");
        let challenge = RepositoryTrustChallenge {
            kind: "ssh-host-key".into(),
            host: "git.example.com".into(),
            port: 22,
            fingerprint_sha256: "SHA256:test".into(),
            key_type: Some("ssh-ed25519".into()),
            subject: None,
            issuer: None,
            not_after: None,
            confirmation_nonce: generate_id("trust-test"),
        };
        remember_pending_trust(challenge.clone());
        let wrong = TrustRepositoryConnectionInput {
            confirmation_nonce: challenge.confirmation_nonce.clone(),
            host: "other.example.com".into(),
            port: 22,
            fingerprint_sha256: challenge.fingerprint_sha256.clone(),
        };
        assert!(trust_ssh_host_key(&conn, wrong).is_err());

        let challenge = RepositoryTrustChallenge {
            confirmation_nonce: generate_id("trust-test"),
            ..challenge
        };
        remember_pending_trust(challenge.clone());
        trust_ssh_host_key(
            &conn,
            TrustRepositoryConnectionInput {
                confirmation_nonce: challenge.confirmation_nonce,
                host: challenge.host.clone(),
                port: challenge.port,
                fingerprint_sha256: challenge.fingerprint_sha256.clone(),
            },
        )
        .expect("trust host key");
        let stored: String = conn
            .query_row(
                "SELECT fingerprint_sha256 FROM plugin_repository_ssh_host_keys
                 WHERE host = ?1 AND port = ?2",
                params![challenge.host, challenge.port as i64],
                |row| row.get(0),
            )
            .expect("read trusted host key");
        assert_eq!(stored, "SHA256:test");
    }

    #[test]
    fn catalog_keeps_an_installed_plugin_on_its_source_repository() {
        let conn = Connection::open_in_memory().expect("open database");
        ensure_plugin_tables(&conn).expect("create plugin tables");
        ensure_repository_tables(&conn).expect("create repository tables");
        let source = add_repository(
            &conn,
            AddRepositoryInput {
                url: "https://source.example.com/plugins.git".into(),
                git_ref: "HEAD".into(),
                index_path: INDEX_FILE.into(),
                display_name: Some("Original".into()),
                authentication_mode: Some("anonymous".into()),
                credential_id: None,
                allow_insecure_transport: false,
                allow_insecure_credentials: false,
            },
        )
        .expect("add source repository");
        let preferred = add_repository(
            &conn,
            AddRepositoryInput {
                url: "https://preferred.example.com/plugins.git".into(),
                git_ref: "HEAD".into(),
                index_path: INDEX_FILE.into(),
                display_name: Some("Preferred".into()),
                authentication_mode: Some("anonymous".into()),
                credential_id: None,
                allow_insecure_transport: false,
                allow_insecure_credentials: false,
            },
        )
        .expect("add preferred repository");
        reorder_repositories(&conn, &[preferred.id.clone(), source.id.clone()])
            .expect("reorder repositories");
        record_installed_version(
            &conn,
            "com.example.fixed",
            "1.0.0",
            "installed-hash",
            None,
            &format!("repository:{}", source.id),
        )
        .expect("record installed plugin");
        conn.execute(
            "UPDATE plugin_versions SET source_repository_id = ?1
             WHERE plugin_id = 'com.example.fixed' AND version = '1.0.0'",
            [&source.id],
        )
        .expect("record fixed source");
        for (repository_id, hash) in [(&source.id, "source-hash"), (&preferred.id, "other-hash")] {
            conn.execute(
                "INSERT INTO plugin_repository_plugins (
                   repository_id, plugin_id, plugin_root, version, package_hash, name,
                   kind, categories_json, platforms_json,
                   engine_tempo, engine_plugin_api, requires_node_runtime,
                   compatible, source_commit, indexed_at
                 ) VALUES (?1, 'com.example.fixed', 'plugins/fixed', '2.0.0', ?2,
                           'Fixed Plugin', 'ui', '[]', '[]', '*', '*', 0, 1,
                           '0123456789012345678901234567890123456789', ?3)",
                params![repository_id, hash, chrono::Utc::now().to_rfc3339()],
            )
            .expect("insert catalog source");
        }
        let catalog = list_catalog_plugins(&conn, None).expect("list catalog");
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].repository_id, source.id);
        assert_eq!(catalog[0].action, "update");
    }
}
