use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::plugins::bridge::HOST_API_VERSION;
use crate::plugins::manifest::PluginManifest;

use super::types::CreateProjectArgs;

const DEFAULT_CATALOG_URL: &str = "https://joooooooojo.github.io/tempo/plugin-assets/catalog.json";
const CATALOG_URL_ENV: &str = "TEMPO_PLUGIN_TEMPLATE_CATALOG_URL";
const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEMPLATE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TEMPLATE_FILES: usize = 256;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteTemplateCatalog {
    catalog_version: u32,
    releases: Vec<RemoteTemplateRelease>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteTemplateRelease {
    version: String,
    min_plugin_api: String,
    manifest_schema: RemoteAsset,
    templates: HashMap<String, RemoteTemplate>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteTemplate {
    files: Vec<RemoteTemplateFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteAsset {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteTemplateFile {
    path: String,
    url: String,
    sha256: String,
    size: u64,
    #[serde(default)]
    render: bool,
}

struct DownloadedTemplateFile {
    path: PathBuf,
    bytes: Vec<u8>,
    render: bool,
}

fn package_name(plugin_id: &str) -> String {
    plugin_id
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn render(source: &str, args: &CreateProjectArgs, schema_url: &str) -> String {
    source
        .replace("__PLUGIN_ID__", args.plugin_id.trim())
        .replace("__PLUGIN_NAME__", args.name.trim())
        .replace("__PACKAGE_NAME__", &package_name(&args.plugin_id))
        .replace("__MANIFEST_SCHEMA_URL__", schema_url)
}

fn parse_catalog(bytes: &[u8]) -> Result<RemoteTemplateCatalog, String> {
    let catalog: RemoteTemplateCatalog =
        serde_json::from_slice(bytes).map_err(|error| format!("模板目录 JSON 无效: {error}"))?;
    if catalog.catalog_version != 1 {
        return Err(format!("不支持模板目录版本 {}", catalog.catalog_version));
    }
    if catalog.releases.is_empty() {
        return Err("模板目录没有可用 release".into());
    }
    Ok(catalog)
}

fn select_release<'a>(
    catalog: &'a RemoteTemplateCatalog,
    kind: &str,
    host_api: &Version,
) -> Result<&'a RemoteTemplateRelease, String> {
    if !matches!(kind, "ui" | "hybrid" | "headless") {
        return Err("插件模板必须是 ui、hybrid 或 headless".into());
    }
    let mut candidates = Vec::new();
    for release in &catalog.releases {
        if !release.templates.contains_key(kind) {
            continue;
        }
        let version = Version::parse(&release.version)
            .map_err(|error| format!("模板版本 {} 无效: {error}", release.version))?;
        let minimum = Version::parse(&release.min_plugin_api).map_err(|error| {
            format!(
                "模板 {} 的 minPluginApi {} 无效: {error}",
                release.version, release.min_plugin_api
            )
        })?;
        if minimum <= *host_api {
            candidates.push((version, release));
        }
    }
    candidates
        .into_iter()
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, release)| release)
        .ok_or_else(|| format!("远端没有兼容 Host API {host_api} 的 {kind} 模板，请更新 Tempo"))
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() || raw.contains('\\') {
        return Err(format!("模板文件路径无效: {raw}"));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("模板文件路径必须是安全的相对路径: {raw}"));
    }
    Ok(path.to_path_buf())
}

fn validate_asset(asset: &RemoteAsset) -> Result<(), String> {
    if asset.size > MAX_ASSET_BYTES as u64 {
        return Err(format!("远端资源大小无效: {} bytes", asset.size));
    }
    if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("远端资源 SHA-256 无效".into());
    }
    Ok(())
}

fn resolve_asset_url(catalog_url: &reqwest::Url, raw: &str) -> Result<reqwest::Url, String> {
    let url = catalog_url
        .join(raw)
        .map_err(|error| format!("远端资源 URL 无效 {raw}: {error}"))?;
    let http_loopback =
        url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !http_loopback {
        return Err(format!("远端资源必须使用 HTTPS: {url}"));
    }
    Ok(url)
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn write_cache(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建模板缓存失败 {}: {error}", parent.display()))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("写入模板缓存失败 {}: {error}", path.display()))
}

async fn fetch_bytes(
    client: &reqwest::Client,
    url: &reqwest::Url,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| format!("下载 {url} 失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("下载 {url} 失败: {error}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err(format!("远端资源超过大小限制: {url}"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取 {url} 失败: {error}"))?;
    if bytes.len() > maximum {
        return Err(format!("远端资源超过大小限制: {url}"));
    }
    Ok(bytes.to_vec())
}

async fn load_catalog(
    client: &reqwest::Client,
    catalog_url: &reqwest::Url,
    cache_root: &Path,
) -> Result<RemoteTemplateCatalog, String> {
    let cache_path = cache_root.join("catalog.json");
    let remote = match fetch_bytes(client, catalog_url, MAX_CATALOG_BYTES).await {
        Ok(bytes) => match parse_catalog(&bytes) {
            Ok(catalog) => {
                write_cache(&cache_path, &bytes)?;
                return Ok(catalog);
            }
            Err(error) => error,
        },
        Err(error) => error,
    };
    let cached = std::fs::read(&cache_path).map_err(|cache_error| {
        format!("无法获取远端模板目录，且没有可用缓存。远端: {remote}；缓存: {cache_error}")
    })?;
    parse_catalog(&cached)
        .map_err(|cache_error| format!("远端模板目录不可用: {remote}；缓存: {cache_error}"))
}

async fn load_asset(
    client: &reqwest::Client,
    url: &reqwest::Url,
    asset: &RemoteAsset,
    cache_path: &Path,
) -> Result<Vec<u8>, String> {
    validate_asset(asset)?;
    if let Ok(bytes) = std::fs::read(cache_path) {
        if bytes.len() as u64 == asset.size && sha256(&bytes).eq_ignore_ascii_case(&asset.sha256) {
            return Ok(bytes);
        }
    }
    let bytes = fetch_bytes(client, url, MAX_ASSET_BYTES).await?;
    if bytes.len() as u64 != asset.size {
        return Err(format!(
            "远端资源大小不匹配 {url}: expected {}, got {}",
            asset.size,
            bytes.len()
        ));
    }
    let actual = sha256(&bytes);
    if !actual.eq_ignore_ascii_case(&asset.sha256) {
        return Err(format!(
            "远端资源 SHA-256 不匹配 {url}: expected {}, got {actual}",
            asset.sha256
        ));
    }
    write_cache(cache_path, &bytes)?;
    Ok(bytes)
}

fn file_asset(file: &RemoteTemplateFile) -> RemoteAsset {
    RemoteAsset {
        url: file.url.clone(),
        sha256: file.sha256.clone(),
        size: file.size,
    }
}

async fn download_template(
    client: &reqwest::Client,
    catalog_url: &reqwest::Url,
    cache_root: &Path,
    release: &RemoteTemplateRelease,
    kind: &str,
) -> Result<(String, Vec<DownloadedTemplateFile>), String> {
    let release_version = Version::parse(&release.version)
        .map_err(|error| format!("模板版本 {} 无效: {error}", release.version))?
        .to_string();
    let schema_url = resolve_asset_url(catalog_url, &release.manifest_schema.url)?;
    let schema_cache = cache_root
        .join("releases")
        .join(&release_version)
        .join("plugin-manifest.schema.json");
    let schema_bytes =
        load_asset(client, &schema_url, &release.manifest_schema, &schema_cache).await?;
    let schema: serde_json::Value = serde_json::from_slice(&schema_bytes)
        .map_err(|error| format!("远端 Manifest Schema JSON 无效: {error}"))?;
    jsonschema::validator_for(&schema)
        .map_err(|error| format!("远端 Manifest Schema 无效: {error}"))?;

    let template = release
        .templates
        .get(kind)
        .ok_or_else(|| format!("模板 {} 不包含 {kind}", release.version))?;
    if template.files.is_empty() || template.files.len() > MAX_TEMPLATE_FILES {
        return Err(format!("模板 {} 文件数量无效", release.version));
    }
    let total = template.files.iter().try_fold(0_u64, |total, file| {
        total
            .checked_add(file.size)
            .ok_or_else(|| "模板文件总大小溢出".to_string())
    })?;
    if total > MAX_TEMPLATE_BYTES {
        return Err(format!("模板 {} 超过大小限制", release.version));
    }

    let mut paths = HashSet::new();
    let mut files = Vec::with_capacity(template.files.len());
    for file in &template.files {
        let relative = safe_relative_path(&file.path)?;
        if !paths.insert(relative.clone()) {
            return Err(format!("模板包含重复文件路径: {}", file.path));
        }
        let asset = file_asset(file);
        let url = resolve_asset_url(catalog_url, &asset.url)?;
        let cache_path = cache_root
            .join("releases")
            .join(&release_version)
            .join(kind)
            .join(&relative);
        let bytes = load_asset(client, &url, &asset, &cache_path).await?;
        files.push(DownloadedTemplateFile {
            path: relative,
            bytes,
            render: file.render,
        });
    }
    Ok((schema_url.to_string(), files))
}

fn scaffold_downloaded(
    root: &Path,
    args: &CreateProjectArgs,
    schema_url: &str,
    files: Vec<DownloadedTemplateFile>,
) -> Result<(), String> {
    let mut rendered = Vec::with_capacity(files.len());
    for file in files {
        let bytes = if file.render {
            let source = String::from_utf8(file.bytes)
                .map_err(|_| format!("需要渲染的模板文件不是 UTF-8: {}", file.path.display()))?;
            render(&source, args, schema_url).into_bytes()
        } else {
            file.bytes
        };
        rendered.push((file.path, bytes));
    }

    let manifest_bytes = rendered
        .iter()
        .find(|(path, _)| path == Path::new("manifest.json"))
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| "远端模板缺少 manifest.json".to_string())?;
    let manifest_raw = std::str::from_utf8(manifest_bytes)
        .map_err(|_| "远端模板 manifest.json 不是 UTF-8".to_string())?;
    let manifest_value: serde_json::Value = serde_json::from_str(manifest_raw)
        .map_err(|error| format!("远端模板 manifest.json 无效: {error}"))?;
    if manifest_value
        .get("$schema")
        .and_then(|value| value.as_str())
        != Some(schema_url)
    {
        return Err("远端模板没有协商正确的 Manifest Schema 地址".into());
    }
    PluginManifest::parse_str(manifest_raw)?;

    for (relative, _) in &rendered {
        let target = root.join(relative);
        if target.exists() {
            return Err(format!("模板目标已存在，未覆盖: {}", target.display()));
        }
    }
    for (relative, bytes) in rendered {
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("创建模板目录失败 {}: {error}", parent.display()))?;
        }
        std::fs::write(&target, bytes)
            .map_err(|error| format!("写入模板文件失败 {}: {error}", target.display()))?;
    }
    Ok(())
}

async fn scaffold_project_from_catalog(
    root: &Path,
    args: &CreateProjectArgs,
    cache_root: &Path,
    catalog_url: reqwest::Url,
) -> Result<(), String> {
    resolve_asset_url(&catalog_url, catalog_url.as_str())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .user_agent("Tempo Plugin Development Assistant")
        .build()
        .map_err(|error| format!("创建模板下载客户端失败: {error}"))?;
    let catalog = load_catalog(&client, &catalog_url, cache_root).await?;
    let host_api =
        Version::parse(HOST_API_VERSION).map_err(|error| format!("Host API 版本无效: {error}"))?;
    let release = select_release(&catalog, &args.kind, &host_api)?;
    let (schema_url, files) =
        download_template(&client, &catalog_url, cache_root, release, &args.kind).await?;
    scaffold_downloaded(root, args, &schema_url, files)
}

pub(super) async fn scaffold_project(
    app: &AppHandle,
    root: &Path,
    args: &CreateProjectArgs,
) -> Result<(), String> {
    let catalog_url_raw =
        std::env::var(CATALOG_URL_ENV).unwrap_or_else(|_| DEFAULT_CATALOG_URL.to_string());
    let catalog_url = reqwest::Url::parse(catalog_url_raw.trim())
        .map_err(|error| format!("模板目录 URL 无效: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("读取应用缓存目录失败: {error}"))?
        .join("plugin-development")
        .join("templates");
    scaffold_project_from_catalog(root, args, &cache_root, catalog_url).await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::extract::{Request, State};
    use axum::http::StatusCode;
    use axum::response::Response;
    use axum::Router;
    use semver::Version;

    use super::{
        safe_relative_path, scaffold_downloaded, scaffold_project_from_catalog, select_release,
        DownloadedTemplateFile, RemoteAsset, RemoteTemplate, RemoteTemplateCatalog,
        RemoteTemplateRelease,
    };
    use crate::builtin_plugins::plugin_dev::commands::types::CreateProjectArgs;

    fn release(version: &str, minimum: &str) -> RemoteTemplateRelease {
        RemoteTemplateRelease {
            version: version.into(),
            min_plugin_api: minimum.into(),
            manifest_schema: RemoteAsset {
                url: "schema.json".into(),
                sha256: "0".repeat(64),
                size: 1,
            },
            templates: HashMap::from([("ui".into(), RemoteTemplate { files: Vec::new() })]),
        }
    }

    #[test]
    fn selects_latest_compatible_template_release() {
        let catalog = RemoteTemplateCatalog {
            catalog_version: 1,
            releases: vec![
                release("0.9.0", "0.9.0"),
                release("1.0.0", "1.0.0"),
                release("2.0.0", "1.1.0"),
            ],
        };
        let selected = select_release(&catalog, "ui", &Version::parse("1.0.0").unwrap()).unwrap();
        assert_eq!(selected.version, "1.0.0");
    }

    #[test]
    fn rejects_unsafe_remote_template_paths() {
        for path in ["", "../manifest.json", "/manifest.json", "src\\main.ts"] {
            assert!(safe_relative_path(path).is_err(), "{path}");
        }
        assert_eq!(
            safe_relative_path("src/ui/main.ts").unwrap(),
            std::path::PathBuf::from("src/ui/main.ts")
        );
    }

    #[test]
    fn scaffolding_injects_the_negotiated_schema_url() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tempo-remote-template-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let args = CreateProjectArgs {
            root_path: root.to_string_lossy().into_owned(),
            plugin_id: "com.example.remote".into(),
            name: "Remote Template".into(),
            kind: "ui".into(),
        };
        let manifest = r#"{
  "$schema": "__MANIFEST_SCHEMA_URL__",
  "manifestVersion": 1,
  "id": "__PLUGIN_ID__",
  "name": "__PLUGIN_NAME__",
  "version": "0.1.0",
  "engines": { "tempo": ">=2", "pluginApi": "^1.0.0" },
  "kind": "ui",
  "contributes": {
    "apps": [{ "id": "main", "name": "Main", "entry": "index.html" }]
  }
}"#;
        let files = vec![
            DownloadedTemplateFile {
                path: "manifest.json".into(),
                bytes: manifest.as_bytes().to_vec(),
                render: true,
            },
            DownloadedTemplateFile {
                path: "index.html".into(),
                bytes: b"<!doctype html>".to_vec(),
                render: false,
            },
        ];
        let schema_url = "https://example.com/templates/1.0.0/schema.json";
        scaffold_downloaded(&root, &args, schema_url, files).unwrap();
        let raw = std::fs::read_to_string(root.join("manifest.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["$schema"], schema_url);
        assert_eq!(value["id"], "com.example.remote");
        std::fs::remove_dir_all(root).unwrap();
    }

    async fn serve_asset(State(root): State<PathBuf>, request: Request) -> Response {
        let relative = request.uri().path().trim_start_matches('/');
        let Ok(relative) = safe_relative_path(relative) else {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(axum::body::Body::empty())
                .unwrap();
        };
        match tokio::fs::read(root.join(relative)).await {
            Ok(bytes) => Response::new(axum::body::Body::from(bytes)),
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::empty())
                .unwrap(),
        }
    }

    #[tokio::test]
    async fn downloads_and_scaffolds_the_published_hybrid_template() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("docs/public/plugin-assets");
        assert!(assets.join("catalog.json").is_file());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().fallback(serve_asset).with_state(assets);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "tempo-remote-download-{}-{unique}",
            std::process::id()
        ));
        let root = base.join("project");
        let cache = base.join("cache");
        std::fs::create_dir_all(&root).unwrap();
        let args = CreateProjectArgs {
            root_path: root.to_string_lossy().into_owned(),
            plugin_id: "com.example.hybrid".into(),
            name: "Remote Hybrid".into(),
            kind: "hybrid".into(),
        };
        let catalog_url = reqwest::Url::parse(&format!("http://{address}/catalog.json")).unwrap();
        scaffold_project_from_catalog(&root, &args, &cache, catalog_url)
            .await
            .unwrap();

        assert!(root.join("src/ui/main.ts").is_file());
        assert!(root.join("src/runtime/main.ts").is_file());
        assert!(root.join("src/runtime/tempo.d.ts").is_file());
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["id"], "com.example.hybrid");
        assert!(manifest["$schema"]
            .as_str()
            .unwrap()
            .starts_with(&format!("http://{address}/releases/1.0.0/")));

        server.abort();
        let _ = server.await;

        let cached_root = base.join("cached-project");
        std::fs::create_dir_all(&cached_root).unwrap();
        let cached_args = CreateProjectArgs {
            root_path: cached_root.to_string_lossy().into_owned(),
            plugin_id: "com.example.cached".into(),
            name: "Cached Hybrid".into(),
            kind: "hybrid".into(),
        };
        let catalog_url = reqwest::Url::parse(&format!("http://{address}/catalog.json")).unwrap();
        scaffold_project_from_catalog(&cached_root, &cached_args, &cache, catalog_url)
            .await
            .unwrap();
        assert!(cached_root.join("src/runtime/main.ts").is_file());

        std::fs::remove_dir_all(base).unwrap();
    }
}
