use std::collections::HashMap;
use crate::db::AppState;

use super::providers::*;
use super::support::*;
use super::types::*;

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
