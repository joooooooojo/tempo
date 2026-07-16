use crate::db::{get_setting, set_setting, AppState};
use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use md5::Md5;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const CONFIG_KEY: &str = "tools_translate_config";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateProviderCreds {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateConfig {
    pub default_provider: String,
    pub default_source_lang: String,
    pub default_target_lang: String,
    pub compare_mode: bool,
    pub providers: HashMap<String, TranslateProviderCreds>,
}

impl Default for TranslateConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        for id in ["youdao", "baidu", "tencent", "google", "deepl"] {
            providers.insert(
                id.to_string(),
                TranslateProviderCreds {
                    enabled: false,
                    fields: HashMap::new(),
                },
            );
        }
        Self {
            default_provider: "youdao".into(),
            default_source_lang: "auto".into(),
            default_target_lang: "zh".into(),
            compare_mode: false,
            providers,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub provider: String,
    pub text: String,
    pub detected_from: Option<String>,
    pub error: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_config(conn: &rusqlite::Connection) -> TranslateConfig {
    let raw = get_setting(conn, CONFIG_KEY, "");
    if raw.trim().is_empty() {
        return TranslateConfig::default();
    }
    match serde_json::from_str::<TranslateConfig>(&raw) {
        Ok(mut cfg) => {
            let defaults = TranslateConfig::default();
            for (id, creds) in defaults.providers {
                cfg.providers.entry(id).or_insert(creds);
            }
            cfg
        }
        Err(_) => TranslateConfig::default(),
    }
}

fn save_config(conn: &rusqlite::Connection, cfg: &TranslateConfig) -> Result<(), String> {
    let raw = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    set_setting(conn, CONFIG_KEY, &raw);
    Ok(())
}

fn field<'a>(creds: &'a TranslateProviderCreds, key: &str) -> Result<&'a str, String> {
    creds
        .fields
        .get(key)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("请先配置「{key}」"))
}

fn truncate_youdao_q(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    if chars.len() <= 20 {
        q.to_string()
    } else {
        let head: String = chars[..10].iter().collect();
        let tail: String = chars[chars.len() - 10..].iter().collect();
        format!("{}{}{}", head, chars.len(), tail)
    }
}

fn map_lang(provider: &str, lang: &str) -> String {
    let lang = lang.trim();
    if lang.is_empty() || lang == "auto" {
        return match provider {
            "google" | "deepl" => String::new(),
            "tencent" => "auto".into(),
            _ => "auto".into(),
        };
    }
    match (provider, lang) {
        ("baidu", "zh") => "zh".into(),
        ("baidu", "en") => "en".into(),
        ("baidu", "ja") => "jp".into(),
        ("baidu", "ko") => "kor".into(),
        ("baidu", "fr") => "fra".into(),
        ("baidu", "es") => "spa".into(),
        ("baidu", "ru") => "ru".into(),
        ("baidu", "de") => "de".into(),
        ("youdao", "zh") => "zh-CHS".into(),
        ("tencent", "zh") => "zh".into(),
        ("google", "zh") => "zh-CN".into(),
        ("deepl", "zh") => "ZH".into(),
        ("deepl", "en") => "EN".into(),
        ("deepl", "ja") => "JA".into(),
        ("deepl", "ko") => "KO".into(),
        ("deepl", "fr") => "FR".into(),
        ("deepl", "es") => "ES".into(),
        ("deepl", "ru") => "RU".into(),
        ("deepl", "de") => "DE".into(),
        _ => lang.to_string(),
    }
}

async fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

async fn translate_youdao(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let app_key = field(creds, "appKey")?;
    let app_secret = field(creds, "appSecret")?;
    // Official docs: salt 最好为 UUID，配合 curtime 防重放（错误码 207）
    let salt = {
        let mut rng = rand::thread_rng();
        format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            rng.gen::<u32>(),
            rng.gen::<u16>(),
            (rng.gen::<u16>() & 0x0fff) | 0x4000,
            (rng.gen::<u16>() & 0x3fff) | 0x8000,
            rng.gen::<u64>() & 0xffffffffffff
        )
    };
    let curtime = now_secs().to_string();
    let sign_str = format!(
        "{}{}{}{}{}",
        app_key,
        truncate_youdao_q(text),
        salt,
        curtime,
        app_secret
    );
    let sign = hex::encode(Sha256::digest(sign_str.as_bytes()));

    let params = [
        ("q", text),
        ("from", from),
        ("to", to),
        ("appKey", app_key),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
        ("signType", "v3"),
        ("curtime", curtime.as_str()),
    ];

    let resp = client
        .post("https://openapi.youdao.com/api")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("有道请求失败: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("有道响应解析失败: {e}"))?;
    let error_code = body.get("errorCode").and_then(|v| v.as_str()).unwrap_or("0");
    if error_code != "0" {
        return Err(format!("有道错误码: {error_code}"));
    }
    let translated = body
        .get("translation")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "有道未返回译文".to_string())?;
    Ok(TranslateResult {
        provider: "youdao".into(),
        text: translated,
        detected_from: body
            .get("l")
            .and_then(|v| v.as_str())
            .map(|s| s.split('2').next().unwrap_or(s).to_string()),
        error: None,
    })
}

async fn translate_baidu(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let app_id = field(creds, "appId")?;
    let secret = field(creds, "secret")?;
    let salt = format!("{}", rand::thread_rng().gen::<u32>());
    let sign_raw = format!("{app_id}{text}{salt}{secret}");
    let sign = hex::encode(Md5::digest(sign_raw.as_bytes()));

    let resp = client
        .get("https://fanyi-api.baidu.com/api/trans/vip/translate")
        .query(&[
            ("q", text),
            ("from", from),
            ("to", to),
            ("appid", app_id),
            ("salt", salt.as_str()),
            ("sign", sign.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("百度请求失败: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("百度响应解析失败: {e}"))?;
    if let Some(err) = body.get("error_code") {
        let msg = body
            .get("error_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(format!("百度错误 {err}: {msg}"));
    }
    let translated = body
        .get("trans_result")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("dst").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "百度未返回译文".to_string())?;
    Ok(TranslateResult {
        provider: "baidu".into(),
        text: translated,
        detected_from: body.get("from").and_then(|v| v.as_str()).map(str::to_string),
        error: None,
    })
}

async fn translate_google(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let api_key = field(creds, "apiKey")?;
    // Official Basic API examples use `q` as a string array.
    let mut body = json!({
        "q": [text],
        "target": to,
        "format": "text",
    });
    if !from.is_empty() && from != "auto" {
        body["source"] = json!(from);
    }
    let resp = client
        .post("https://translation.googleapis.com/language/translate/v2")
        .query(&[("key", api_key)])
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Google 请求失败: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| format!("Google 响应解析失败: {e}"))?;
    if !status.is_success() {
        let msg = body
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("请求失败");
        return Err(format!("Google 错误: {msg}"));
    }
    let translations = body
        .pointer("/data/translations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Google 未返回译文".to_string())?;
    let text_out = translations
        .iter()
        .filter_map(|item| item.get("translatedText").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    let detected = translations
        .first()
        .and_then(|item| item.get("detectedSourceLanguage"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(TranslateResult {
        provider: "google".into(),
        text: text_out,
        detected_from: detected,
        error: None,
    })
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

/// Tencent Cloud TC3-HMAC-SHA256 for TMT TextTranslate.
async fn translate_tencent(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let secret_id = field(creds, "secretId")?;
    let secret_key = field(creds, "secretKey")?;
    let region = creds
        .fields
        .get("region")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("ap-guangzhou");

    let host = "tmt.tencentcloudapi.com";
    let service = "tmt";
    let action = "TextTranslate";
    let version = "2018-03-21";
    let algorithm = "TC3-HMAC-SHA256";
    let timestamp = now_secs();
    // TC3 CredentialScope 的 Date 必须是 UTC 日期（与 X-TC-Timestamp 一致）
    let date = Utc
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into());

    let payload = json!({
        "SourceText": text,
        "Source": from,
        "Target": to,
        "ProjectId": 0,
    })
    .to_string();

    let hashed_payload = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:application/json; charset=utf-8\nhost:{host}\nx-tc-action:{}\n",
        action.to_lowercase()
    );
    let signed_headers = "content-type;host;x-tc-action";
    let canonical_request = format!(
        "POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}"
    );
    let credential_scope = format!("{date}/{service}/tc3_request");
    let string_to_sign = format!(
        "{algorithm}\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hmac_sha256_hex(&secret_signing, string_to_sign.as_bytes());

    let authorization = format!(
        "{algorithm} Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let resp = client
        .post(format!("https://{host}"))
        .header("Authorization", authorization)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", host)
        .header("X-TC-Action", action)
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("X-TC-Version", version)
        .header("X-TC-Region", region)
        .body(payload)
        .send()
        .await
        .map_err(|e| format!("腾讯请求失败: {e}"))?;

    let body: Value = resp.json().await.map_err(|e| format!("腾讯响应解析失败: {e}"))?;
    if let Some(err) = body.pointer("/Response/Error") {
        let msg = err
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        let code = err.get("Code").and_then(|v| v.as_str()).unwrap_or("?");
        return Err(format!("腾讯错误 {code}: {msg}"));
    }
    let translated = body
        .pointer("/Response/TargetText")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "腾讯未返回译文".to_string())?
        .to_string();
    Ok(TranslateResult {
        provider: "tencent".into(),
        text: translated,
        detected_from: body
            .pointer("/Response/Source")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        error: None,
    })
}

async fn translate_deepl(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let api_key = field(creds, "apiKey")?;
    let base = if api_key.ends_with(":fx") {
        "https://api-free.deepl.com"
    } else {
        "https://api.deepl.com"
    };

    let mut body = json!({
        "text": [text],
        "target_lang": to,
    });
    if !from.is_empty() && from != "auto" {
        body["source_lang"] = json!(from);
    }

    let resp = client
        .post(format!("{base}/v2/translate"))
        .header("Authorization", format!("DeepL-Auth-Key {api_key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DeepL 请求失败: {e}"))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("DeepL 响应解析失败: {e}"))?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("请求失败");
        return Err(format!("DeepL 错误: {msg}"));
    }

    let first = body
        .get("translations")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| "DeepL 未返回译文".to_string())?;

    let translated = first
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "DeepL 未返回译文".to_string())?
        .to_string();
    let detected = first
        .get("detected_source_language")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(TranslateResult {
        provider: "deepl".into(),
        text: translated,
        detected_from: detected,
        error: None,
    })
}

async fn translate_one(
    provider: &str,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> TranslateResult {
    let client = match http_client().await {
        Ok(c) => c,
        Err(e) => {
            return TranslateResult {
                provider: provider.into(),
                text: String::new(),
                detected_from: None,
                error: Some(e),
            };
        }
    };
    let from_mapped = map_lang(provider, from);
    let to_mapped = map_lang(provider, to);
    let result = match provider {
        "youdao" => translate_youdao(&client, creds, text, &from_mapped, &to_mapped).await,
        "baidu" => translate_baidu(&client, creds, text, &from_mapped, &to_mapped).await,
        "google" => translate_google(&client, creds, text, &from_mapped, &to_mapped).await,
        "tencent" => translate_tencent(&client, creds, text, &from_mapped, &to_mapped).await,
        "deepl" => translate_deepl(&client, creds, text, &from_mapped, &to_mapped).await,
        other => Err(format!("未知引擎: {other}")),
    };
    match result {
        Ok(r) => r,
        Err(e) => TranslateResult {
            provider: provider.into(),
            text: String::new(),
            detected_from: None,
            error: Some(e),
        },
    }
}

#[tauri::command]
pub fn get_translate_config(state: tauri::State<AppState>) -> TranslateConfig {
    let conn = state.db.lock();
    load_config(&conn)
}

#[tauri::command]
pub fn update_translate_config(
    state: tauri::State<AppState>,
    config: TranslateConfig,
) -> Result<TranslateConfig, String> {
    let conn = state.db.lock();
    save_config(&conn, &config)?;
    Ok(config)
}

#[tauri::command]
pub async fn translate_text(
    state: tauri::State<'_, AppState>,
    provider: String,
    text: String,
    from: String,
    to: String,
) -> Result<TranslateResult, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("请输入要翻译的文本".into());
    }
    let creds = {
        let conn = state.db.lock();
        let cfg = load_config(&conn);
        cfg.providers
            .get(&provider)
            .cloned()
            .ok_or_else(|| format!("未找到引擎配置: {provider}"))?
    };
    let result = translate_one(&provider, &creds, &text, &from, &to).await;
    if let Some(err) = &result.error {
        return Err(err.clone());
    }
    Ok(result)
}

#[tauri::command]
pub async fn translate_compare(
    state: tauri::State<'_, AppState>,
    providers: Vec<String>,
    text: String,
    from: String,
    to: String,
) -> Result<Vec<TranslateResult>, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("请输入要翻译的文本".into());
    }
    let cfg = {
        let conn = state.db.lock();
        load_config(&conn)
    };
    let list = if providers.is_empty() {
        cfg.providers
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    } else {
        providers
    };
    if list.is_empty() {
        return Err("请至少启用一个翻译引擎".into());
    }

    let mut handles = Vec::new();
    for provider in list {
        let creds = cfg
            .providers
            .get(&provider)
            .cloned()
            .unwrap_or(TranslateProviderCreds {
                enabled: false,
                fields: HashMap::new(),
            });
        let text = text.clone();
        let from = from.clone();
        let to = to.clone();
        handles.push(tokio::spawn(async move {
            translate_one(&provider, &creds, &text, &from, &to).await
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(TranslateResult {
                provider: "?".into(),
                text: String::new(),
                detected_from: None,
                error: Some(format!("任务失败: {e}")),
            }),
        }
    }
    Ok(results)
}

#[tauri::command]
pub async fn test_translate_provider(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> Result<TranslateResult, String> {
    translate_text(
        state,
        provider,
        "Hello".into(),
        "en".into(),
        "zh".into(),
    )
    .await
}
