use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(super) const CONFIG_KEY: &str = "tools_translate_config";

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

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TranslateStreamEvent {
    Delta { text: String },
    Done { text: String },
    Error { message: String },
}
