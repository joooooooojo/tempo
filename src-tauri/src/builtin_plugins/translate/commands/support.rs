use crate::db::{get_setting, set_setting};
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::*;

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn load_config(conn: &rusqlite::Connection) -> TranslateConfig {
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

pub(super) fn save_config(conn: &rusqlite::Connection, cfg: &TranslateConfig) -> Result<(), String> {
    let raw = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    set_setting(conn, CONFIG_KEY, &raw);
    Ok(())
}

pub(super) fn field<'a>(creds: &'a TranslateProviderCreds, key: &str) -> Result<&'a str, String> {
    creds
        .fields
        .get(key)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("请先配置「{key}」"))
}

pub(super) fn truncate_youdao_q(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    if chars.len() <= 20 {
        q.to_string()
    } else {
        let head: String = chars[..10].iter().collect();
        let tail: String = chars[chars.len() - 10..].iter().collect();
        format!("{}{}{}", head, chars.len(), tail)
    }
}

pub(super) fn map_lang(provider: &str, lang: &str) -> String {
    let lang = lang.trim();
    if lang.is_empty() || lang == "auto" {
        return match provider {
            "google" | "deepl" | "tencent" => String::new(),
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

pub(super) async fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}
