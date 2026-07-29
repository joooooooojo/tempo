import type { TranslateConfig, TranslateProviderId } from "@/types";

export const PROVIDERS: Array<{
  id: TranslateProviderId;
  name: string;
  fields: Array<{ key: string; label: string; secret?: boolean }>;
}> = [
  {
    id: "youdao",
    name: "有道翻译",
    fields: [
      { key: "appKey", label: "应用 ID (appKey)" },
      { key: "appSecret", label: "应用密钥 (appSecret)", secret: true },
    ],
  },
  {
    id: "baidu",
    name: "百度翻译",
    fields: [
      { key: "appId", label: "APP ID" },
      { key: "secret", label: "密钥", secret: true },
    ],
  },
  {
    id: "tencent",
    name: "腾讯翻译",
    fields: [
      { key: "secretId", label: "SecretId" },
      { key: "secretKey", label: "SecretKey", secret: true },
      { key: "region", label: "地域（默认 ap-guangzhou）" },
    ],
  },
  {
    id: "google",
    name: "Google 翻译",
    fields: [{ key: "apiKey", label: "API Key", secret: true }],
  },
  {
    id: "deepl",
    name: "DeepL",
    fields: [{ key: "apiKey", label: "API Key", secret: true }],
  },
];

export const LANGS = [
  { value: "auto", label: "自动检测" },
  { value: "zh", label: "中文" },
  { value: "en", label: "英语" },
  { value: "ja", label: "日语" },
  { value: "ko", label: "韩语" },
  { value: "fr", label: "法语" },
  { value: "es", label: "西班牙语" },
  { value: "ru", label: "俄语" },
  { value: "de", label: "德语" },
];

export const TARGET_LANGS = LANGS.filter((l) => l.value !== "auto");

export const PROVIDER_ITEMS = PROVIDERS.map((p) => ({ value: p.id, label: p.name }));

export const TRANSLATE_DEBOUNCE_MS = 600;

export function providerName(id: string) {
  return PROVIDERS.find((p) => p.id === id)?.name ?? id;
}

export function emptyConfig(): TranslateConfig {
  return {
    defaultProvider: "youdao",
    defaultSourceLang: "auto",
    defaultTargetLang: "zh",
    compareMode: false,
    providers: Object.fromEntries(
      PROVIDERS.map((p) => [p.id, { enabled: false, fields: {} }])
    ),
  };
}
