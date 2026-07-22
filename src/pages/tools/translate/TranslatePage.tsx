import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeftRight,
  Copy,
  Eye,
  EyeOff,
  Loader2,
  Settings2,
} from "lucide-react";
import { toast } from "sonner";
import type { BuiltinAppProps } from "@/apps/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/api";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { TranslateConfig, TranslateProviderId, TranslateResult } from "@/types";

const PROVIDERS: Array<{
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

const LANGS = [
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

const TARGET_LANGS = LANGS.filter((l) => l.value !== "auto");

const PROVIDER_ITEMS = PROVIDERS.map((p) => ({ value: p.id, label: p.name }));

const TRANSLATE_DEBOUNCE_MS = 600;

function providerName(id: string) {
  return PROVIDERS.find((p) => p.id === id)?.name ?? id;
}

function emptyConfig(): TranslateConfig {
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

export function TranslatePage({ initialTranslateText }: BuiltinAppProps) {
  const [config, setConfig] = useState<TranslateConfig>(emptyConfig);
  const [source, setSource] = useState(() => initialTranslateText?.trim() ?? "");
  const [from, setFrom] = useState("auto");
  const [to, setTo] = useState("zh");
  const [provider, setProvider] = useState("youdao");
  const [compareMode, setCompareMode] = useState(false);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<TranslateResult | null>(null);
  const [compareResults, setCompareResults] = useState<TranslateResult[]>([]);
  const [configOpen, setConfigOpen] = useState(false);
  const [draft, setDraft] = useState<TranslateConfig>(emptyConfig);
  const [activeTab, setActiveTab] = useState<TranslateProviderId>("youdao");
  const [showSecrets, setShowSecrets] = useState<Record<string, boolean>>({});
  const [testing, setTesting] = useState(false);
  const [savingConfig, setSavingConfig] = useState(false);
  const sourceRef = useRef<HTMLTextAreaElement>(null);
  const resultRef = useRef<HTMLTextAreaElement>(null);
  const translateRequestId = useRef(0);

  useEffect(() => {
    const el = sourceRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [source]);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const el = sourceRef.current;
      if (!el) return;
      el.focus({ preventScroll: true });
      const end = el.value.length;
      el.setSelectionRange(end, end);
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    const el = resultRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [result?.text]);

  const load = useCallback(async () => {
    try {
      const next = await api.getTranslateConfig();
      setConfig(next);
      setProvider(next.defaultProvider || "youdao");
      setFrom(next.defaultSourceLang || "auto");
      setTo(next.defaultTargetLang || "zh");
      setCompareMode(next.compareMode);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const configuredProviders = useMemo(
    () =>
      PROVIDERS.filter((p) => {
        const creds = config.providers[p.id];
        if (!creds?.enabled) return false;
        return p.fields
          .filter((f) => f.key !== "region")
          .every((f) => (creds.fields[f.key] ?? "").trim().length > 0);
      }),
    [config]
  );

  const openConfig = () => {
    setDraft(structuredClone(config));
    setActiveTab((provider as TranslateProviderId) || "youdao");
    setConfigOpen(true);
  };

  const saveConfig = async () => {
    setSavingConfig(true);
    try {
      const nextDraft: TranslateConfig = {
        ...draft,
        providers: { ...draft.providers },
      };
      for (const p of PROVIDERS) {
        const creds = nextDraft.providers[p.id] ?? { enabled: false, fields: {} };
        const hasKeys = p.fields
          .filter((f) => f.key !== "region")
          .every((f) => (creds.fields[f.key] ?? "").trim().length > 0);
        nextDraft.providers[p.id] = {
          ...creds,
          // Filling keys opts the engine into compare mode by default.
          enabled: hasKeys ? true : false,
        };
      }
      const next = await api.updateTranslateConfig(nextDraft);
      setConfig(next);
      setConfigOpen(false);
      toast.success("密钥已保存到本地");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingConfig(false);
    }
  };

  const testProvider = async () => {
    setTesting(true);
    try {
      await api.updateTranslateConfig(draft);
      const res = await api.testTranslateProvider(activeTab);
      toast.success(`连通成功：${res.text}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setTesting(false);
    }
  };

  const swapLang = () => {
    if (from === "auto") {
      toast.message("源语言为自动检测时无法交换");
      return;
    }
    setFrom(to);
    setTo(from);
  };

  const runTranslate = useCallback(
    async (options?: { silent?: boolean }) => {
      const text = source.trim();
      if (!text) {
        if (!options?.silent) toast.error("请输入原文");
        setResult(null);
        setCompareResults([]);
        return;
      }

      const requestId = ++translateRequestId.current;
      setLoading(true);
      setResult(null);
      setCompareResults([]);

      try {
        if (compareMode) {
          const ids = configuredProviders.map((p) => p.id);
          if (ids.length === 0) {
            if (!options?.silent) toast.error("请先在配置中填写至少一个引擎的密钥");
            return;
          }
          const results = await api.translateCompare(ids, text, from, to);
          if (requestId !== translateRequestId.current) return;
          setCompareResults(results);
          await api.updateTranslateConfig({
            ...config,
            defaultSourceLang: from,
            defaultTargetLang: to,
            compareMode: true,
          });
        } else {
          const res = await api.translateText(provider, text, from, to);
          if (requestId !== translateRequestId.current) return;
          setResult(res);
          await api.updateTranslateConfig({
            ...config,
            defaultProvider: provider,
            defaultSourceLang: from,
            defaultTargetLang: to,
            compareMode: false,
          });
        }
      } catch (error) {
        if (requestId !== translateRequestId.current) return;
        toast.error(error instanceof Error ? error.message : String(error));
      } finally {
        if (requestId === translateRequestId.current) setLoading(false);
      }
    },
    [compareMode, config, configuredProviders, from, provider, source, to]
  );

  useEffect(() => {
    const text = source.trim();
    if (!text) {
      translateRequestId.current += 1;
      setLoading(false);
      setResult(null);
      setCompareResults([]);
      return;
    }

    const timer = window.setTimeout(() => {
      void runTranslate({ silent: true });
    }, TRANSLATE_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [source, from, to, provider, compareMode, configuredProviders, runTranslate]);

  const copyText = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("已复制");
    } catch {
      toast.error("复制失败");
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col gap-3 p-3">
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Select items={LANGS} value={from} onValueChange={(v) => v && setFrom(v)}>
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {LANGS.map((lang) => (
                  <SelectItem key={lang.value} value={lang.value}>
                    {lang.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>

          <Button variant="ghost" size="icon" className="size-8" onClick={swapLang} aria-label="交换语言">
            <ArrowLeftRight className="size-3.5" />
          </Button>

          <Select items={TARGET_LANGS} value={to} onValueChange={(v) => v && setTo(v)}>
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {TARGET_LANGS.map((lang) => (
                  <SelectItem key={lang.value} value={lang.value}>
                    {lang.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>

          {!compareMode && (
            <Select items={PROVIDER_ITEMS} value={provider} onValueChange={(v) => v && setProvider(v)}>
              <SelectTrigger className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {PROVIDERS.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          )}

          <div className="ml-auto flex items-center gap-2">
            <Label htmlFor="compare-mode" className="text-[12px] text-muted-foreground">
              多引擎对比
            </Label>
            <Switch
              id="compare-mode"
              checked={compareMode}
              onCheckedChange={setCompareMode}
            />
          </div>
        </div>

        <div className={cn("grid min-h-0 flex-1 gap-3", compareMode ? "grid-cols-1" : "md:grid-cols-2")}>
          <div className="flex min-h-0 flex-col">
            <div className="mb-2 flex h-8 shrink-0 items-center justify-between gap-2">
              <div className="text-[13px] font-medium text-muted-foreground">原文</div>
              <Button
                variant="ghost"
                size="sm"
                className="invisible pointer-events-none"
                tabIndex={-1}
                aria-hidden
              >
                <Copy className="size-3.5" />
                复制
              </Button>
            </div>
            <ScrollArea
              className="min-h-0 flex-1 rounded-lg border border-border/60 bg-background/50"
              viewportClassName="p-0"
            >
              <textarea
                ref={sourceRef}
                value={source}
                rows={1}
                onChange={(e) => setSource(e.target.value)}
                className={cn(
                  "block w-full resize-none overflow-hidden border-0 bg-transparent px-3 pt-3 pb-3",
                  "min-h-full! text-[13px] leading-6 text-foreground outline-none",
                  "focus-visible:ring-2 focus-visible:ring-primary/20"
                )}
                placeholder="输入要翻译的文本…"
              />
            </ScrollArea>
          </div>

          {!compareMode && (
            <div className="flex min-h-0 flex-col">
              <div className="mb-2 flex h-8 shrink-0 items-center justify-between gap-2">
                <div className="min-w-0 truncate text-[13px] font-medium text-muted-foreground">
                  译文
                  {result?.detectedFrom ? ` · 检测：${result.detectedFrom}` : ""}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  className={cn(!result?.text && "invisible")}
                  disabled={!result?.text}
                  onClick={() => result?.text && void copyText(result.text)}
                >
                  <Copy className="size-3.5" />
                  复制
                </Button>
              </div>
              <ScrollArea
                className="min-h-0 flex-1 rounded-lg border border-border/60 bg-background/30"
                viewportClassName="p-0"
              >
                <textarea
                  ref={resultRef}
                  readOnly
                  rows={1}
                  value={result?.text ?? ""}
                  className={cn(
                    "block w-full resize-none overflow-hidden border-0 bg-transparent px-3 pt-3 pb-3",
                    "min-h-full! text-[13px] leading-6 text-foreground outline-none"
                  )}
                  placeholder="译文将显示在这里"
                />
              </ScrollArea>
            </div>
          )}
        </div>

        {compareMode && (
          <ScrollArea className="min-h-0 flex-1" viewportClassName="p-0">
            <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
              {(compareResults.length > 0
                ? compareResults
                : configuredProviders.map((p) => ({
                    provider: p.id,
                    text: "",
                    error: null,
                  }))
              ).map((item) => (
                <div
                  key={item.provider}
                  className="flex min-h-[160px] flex-col rounded-lg border border-border/60 bg-background/40 p-3"
                >
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="text-[12px] font-medium">{providerName(item.provider)}</span>
                    {item.text && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7"
                        onClick={() => void copyText(item.text)}
                      >
                        <Copy className="size-3.5" />
                      </Button>
                    )}
                  </div>
                  {item.error ? (
                    <p className="text-[12px] text-destructive">{item.error}</p>
                  ) : (
                    <p className="whitespace-pre-wrap text-[13px] leading-6 text-foreground">
                      {item.text || (loading ? "翻译中…" : "等待翻译")}
                    </p>
                  )}
                </div>
              ))}
              {configuredProviders.length === 0 && (
                <p className="text-[12px] text-muted-foreground">请先配置至少一个引擎的密钥。</p>
              )}
            </div>
          </ScrollArea>
        )}
      </div>

      <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border/60 px-4 py-3">
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={openConfig}>
            <Settings2 />
            配置密钥
          </Button>
        </div>
        <div className="flex items-center gap-2">
          <Button onClick={() => void runTranslate()} disabled={loading}>
            {loading ? <Loader2 className="animate-spin" /> : null}
            翻译
          </Button>
        </div>
      </footer>

      <Dialog open={configOpen} onOpenChange={setConfigOpen}>
        <DialogPanel className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>翻译密钥配置</DialogTitle>
          </DialogHeader>
          <DialogContent className="flex max-h-[70vh] min-h-0 flex-col gap-3 overflow-hidden">
            <div className="flex shrink-0 flex-wrap gap-1">
              {PROVIDERS.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={cn(
                    "rounded-md px-2.5 py-1 text-[12px]",
                    activeTab === p.id
                      ? "bg-primary/15 text-primary"
                      : "text-muted-foreground hover:bg-foreground/5"
                  )}
                  onClick={() => setActiveTab(p.id)}
                >
                  {p.name}
                </button>
              ))}
            </div>

            <ScrollArea className="min-h-0 flex-1" viewportClassName="p-1">
              {PROVIDERS.filter((p) => p.id === activeTab).map((p) => {
                const creds = draft.providers[p.id] ?? { enabled: false, fields: {} };
                return (
                  <div key={p.id} className="space-y-3">
                    <div className="flex items-center justify-between">
                      <Label>启用（用于对比模式）</Label>
                      <Switch
                        checked={creds.enabled}
                        onCheckedChange={(checked) =>
                          setDraft((prev) => ({
                            ...prev,
                            providers: {
                              ...prev.providers,
                              [p.id]: { ...creds, enabled: checked },
                            },
                          }))
                        }
                      />
                    </div>
                    {p.fields.map((field) => {
                      const secretKey = `${p.id}.${field.key}`;
                      const show = showSecrets[secretKey];
                      return (
                        <div key={field.key} className="space-y-1.5">
                          <Label>{field.label}</Label>
                          <div className="relative">
                            <Input
                              type={field.secret && !show ? "password" : "text"}
                              value={creds.fields[field.key] ?? ""}
                              onChange={(e) =>
                                setDraft((prev) => ({
                                  ...prev,
                                  providers: {
                                    ...prev.providers,
                                    [p.id]: {
                                      ...creds,
                                      fields: { ...creds.fields, [field.key]: e.target.value },
                                    },
                                  },
                                }))
                              }
                              className={field.secret ? "pr-9" : undefined}
                            />
                            {field.secret && (
                              <button
                                type="button"
                                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground"
                                onClick={() =>
                                  setShowSecrets((prev) => ({ ...prev, [secretKey]: !prev[secretKey] }))
                                }
                              >
                                {show ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </ScrollArea>
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => void testProvider()} disabled={testing}>
              {testing ? <Loader2 className="size-3.5 animate-spin" /> : null}
              测试连通
            </Button>
            <Button onClick={() => void saveConfig()} disabled={savingConfig}>
              {savingConfig ? <Loader2 className="size-3.5 animate-spin" /> : null}
              保存
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </div>
  );
}
