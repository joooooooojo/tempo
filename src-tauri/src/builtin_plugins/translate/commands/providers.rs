use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use md5::Md5;
use rand::Rng;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::support::*;
use super::types::*;

type HmacSha256 = Hmac<Sha256>;

pub(super) async fn translate_youdao(
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

pub(super) async fn translate_baidu(
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

pub(super) async fn translate_google(
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

pub(super) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub(super) fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub(super) fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

/// Non-streaming Hunyuan ChatTranslations (compare / test / plain translate).
pub(super) async fn translate_tencent(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult, String> {
    let (req, payload) = build_hunyuan_request(client, creds, text, from, to, false)?;
    let resp = req
        .body(payload)
        .send()
        .await
        .map_err(|e| format!("混元翻译请求失败: {e}"))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("混元翻译响应解析失败: {e}"))?;
    if let Some(err) = parse_hunyuan_error(&body) {
        return Err(err);
    }
    if !status.is_success() {
        return Err(format!("混元翻译 HTTP {}", status.as_u16()));
    }
    let translated = body
        .pointer("/Response/Choices/0/Message/Content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "混元翻译未返回译文".to_string())?
        .to_string();
    Ok(TranslateResult {
        provider: "tencent".into(),
        text: translated,
        detected_from: None,
        error: None,
    })
}

/// Streaming Hunyuan ChatTranslations (single-engine UI typewriter).
pub(super) async fn translate_tencent_stream<F>(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
    mut on_delta: F,
) -> Result<TranslateResult, String>
where
    F: FnMut(&str) -> Result<(), String>,
{
    let (req, payload) = build_hunyuan_request(client, creds, text, from, to, true)?;
    let mut resp = req
        .header("Accept", "text/event-stream")
        .body(payload)
        .send()
        .await
        .map_err(|e| format!("混元翻译请求失败: {e}"))?;

    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Non-SSE JSON error / unexpected payload
    if content_type.contains("application/json") {
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("混元翻译响应解析失败: {e}"))?;
        return Err(parse_hunyuan_error(&body).unwrap_or_else(|| {
            if !status.is_success() {
                format!("混元翻译 HTTP {}", status.as_u16())
            } else {
                "混元翻译未返回流式译文".into()
            }
        }));
    }

    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    let mut translated = String::new();

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("混元翻译流读取失败: {e}"))?
    {
        byte_buf.extend_from_slice(&chunk);
        let valid_up_to = match std::str::from_utf8(&byte_buf) {
            Ok(s) => s.len(),
            Err(err) => err.valid_up_to(),
        };
        if valid_up_to == 0 {
            continue;
        }
        let piece = std::str::from_utf8(&byte_buf[..valid_up_to])
            .expect("valid_up_to marks a UTF-8 prefix");
        buffer.push_str(piece);
        byte_buf.drain(..valid_up_to);

        for data in drain_sse_data_events(&mut buffer) {
            if data == "[DONE]" {
                continue;
            }
            let body: Value = serde_json::from_str(&data)
                .map_err(|e| format!("混元翻译流解析失败: {e}"))?;
            if let Some(err) = parse_hunyuan_error(&body) {
                return Err(err);
            }
            let delta = body
                .pointer("/Response/Choices/0/Delta/Content")
                .or_else(|| body.pointer("/Choices/0/Delta/Content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !delta.is_empty() {
                translated.push_str(delta);
                on_delta(delta)?;
            }
        }
    }

    if translated.is_empty() {
        return Err("混元翻译未返回译文".into());
    }

    Ok(TranslateResult {
        provider: "tencent".into(),
        text: translated,
        detected_from: None,
        error: None,
    })
}

fn build_hunyuan_request(
    client: &reqwest::Client,
    creds: &TranslateProviderCreds,
    text: &str,
    from: &str,
    to: &str,
    stream: bool,
) -> Result<(reqwest::RequestBuilder, String), String> {
    let secret_id = field(creds, "secretId")?;
    let secret_key = field(creds, "secretKey")?;
    let region = creds
        .fields
        .get("region")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let model = creds
        .fields
        .get("model")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("hunyuan-translation");

    let host = "hunyuan.tencentcloudapi.com";
    let service = "hunyuan";
    let action = "ChatTranslations";
    let version = "2023-09-01";
    let algorithm = "TC3-HMAC-SHA256";
    let timestamp = now_secs();
    // TC3 CredentialScope 的 Date 必须是 UTC 日期（与 X-TC-Timestamp 一致）
    let date = Utc
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into());

    let mut payload = json!({
        "Model": model,
        "Stream": stream,
        "Text": text,
        "Target": to,
    });
    if !from.is_empty() && from != "auto" {
        payload["Source"] = json!(from);
    }
    let payload = payload.to_string();

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

    let mut req = client
        .post(format!("https://{host}"))
        .header("Authorization", authorization)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", host)
        .header("X-TC-Action", action)
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("X-TC-Version", version);
    if let Some(region) = region {
        req = req.header("X-TC-Region", region);
    }
    Ok((req, payload))
}

fn parse_hunyuan_error(body: &Value) -> Option<String> {
    if let Some(err) = body
        .pointer("/Response/Error")
        .filter(|v| v.is_object())
    {
        let msg = err
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        let code = err.get("Code").and_then(|v| v.as_str()).unwrap_or("?");
        return Some(format!("混元翻译错误 {code}: {msg}"));
    }
    if let Some(err) = body
        .pointer("/Response/ErrorMsg")
        .filter(|v| v.is_object())
    {
        let msg = err
            .get("Msg")
            .or_else(|| err.get("Message"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Some(format!("混元翻译错误: {msg}"));
    }
    None
}

fn drain_sse_data_events(buffer: &mut String) -> Vec<String> {
    let mut events = Vec::new();
    loop {
        let sep = if let Some(i) = buffer.find("\r\n\r\n") {
            (i, 4)
        } else if let Some(i) = buffer.find("\n\n") {
            (i, 2)
        } else {
            break;
        };
        let raw = buffer[..sep.0].to_string();
        *buffer = buffer[sep.0 + sep.1..].to_string();
        let mut data_lines = Vec::new();
        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if !data_lines.is_empty() {
            events.push(data_lines.join("\n"));
        }
    }
    events
}

pub(super) async fn translate_deepl(
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

pub(super) async fn translate_one(
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

