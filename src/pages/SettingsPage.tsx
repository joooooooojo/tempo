import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { emit } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, RefreshCw, RotateCcw, Copy, Eye, EyeOff, CircleHelp } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { api } from "@/lib/api";
import { emitThemeChange } from "@/lib/theme";
import { getAppVersion } from "@/lib/update";
import { useUpdateStore, runCheckUpdate, runInstallUpdate } from "@/lib/updateStore";
import {
  DEFAULT_SHORTCUTS,
  formatShortcutLabel,
  shortcutFromKeyboardEvent,
} from "@/lib/shortcut";
import type { Settings } from "@/types";

const CLIPBOARD_RETENTION_OPTIONS = [
  { value: "days", label: "天" },
  { value: "weeks", label: "周" },
  { value: "months", label: "个月" },
  { value: "years", label: "年" },
  { value: "permanent", label: "永久" },
] as const satisfies ReadonlyArray<{
  value: Settings["clipboard_history_retention"];
  label: string;
}>;

const THEME_OPTIONS: Array<{ value: Settings["theme"]; label: string }> = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

const SHORTCUT_SETTING_KEYS = [
  "shortcut_quick_todo",
  "shortcut_clipboard_picker",
  "shortcut_snippet_picker",
] as const;

type ShortcutSettingKey = (typeof SHORTCUT_SETTING_KEYS)[number];

function normalizeShortcutForComparison(shortcut: string) {
  return shortcut.trim().toLowerCase().replace(/^ctrl\+/, "control+");
}

function clipboardRetentionIndex(value: Settings["clipboard_history_retention"]) {
  const index = CLIPBOARD_RETENTION_OPTIONS.findIndex((option) => option.value === value);
  return index >= 0 ? index : 0;
}

function clipboardRetentionValue(index: number): Settings["clipboard_history_retention"] {
  return CLIPBOARD_RETENTION_OPTIONS[index]?.value ?? "days";
}

function clipboardRetentionLabel(value: Settings["clipboard_history_retention"]) {
  switch (value) {
    case "days":
      return "保留最近 1 天内的历史";
    case "weeks":
      return "保留最近 1 周内的历史";
    case "months":
      return "保留最近 1 个月内的历史";
    case "years":
      return "保留最近 1 年内的历史";
    case "permanent":
      return "永久保留历史记录";
    default:
      return "保留最近 1 天内的历史";
  }
}

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [migratingStorage, setMigratingStorage] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const {
    checking: checkingUpdate,
    applying: applyingUpdate,
    progress: updateProgress,
    pendingUpdate,
    pendingVersion,
  } = useUpdateStore();
  const [showMcpToken, setShowMcpToken] = useState(false);
  const [mcpPortDraft, setMcpPortDraft] = useState("");

  const load = async () => {
    const s = await api.getSettings();
    setSettings(s);
    setMcpPortDraft(String(s.mcp_port));
  };

  useEffect(() => {
    load().catch(console.error);
    getAppVersion().then(setAppVersion).catch(console.error);
  }, []);

  const update = async (patch: Partial<Settings>) => {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, ...patch });
    try {
      await api.updateSettings(patch);
      if (patch.theme !== undefined) {
        await emitThemeChange(patch.theme);
      }
      toast.success("已保存");
    } catch (error) {
      setSettings(previous);
      toast.error(error instanceof Error ? error.message : String(error));
      throw error;
    }
  };

  const updateShortcut = (key: ShortcutSettingKey, value: string) => {
    if (!settings) return Promise.resolve();

    const patch: Partial<Settings> = { [key]: value };
    const normalized = normalizeShortcutForComparison(value);
    if (normalized) {
      for (const otherKey of SHORTCUT_SETTING_KEYS) {
        if (
          otherKey !== key &&
          normalizeShortcutForComparison(settings[otherKey]) === normalized
        ) {
          patch[otherKey] = "";
        }
      }
    }
    return update(patch);
  };

  const changeStorageDir = async () => {
    if (migratingStorage) return;

    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择文件存储位置",
      });
      if (!selected || Array.isArray(selected)) return;

      setMigratingStorage(true);
      const nextSettings = await api.setStorageDir(selected);
      setSettings(nextSettings);
      await load();
      toast.success("文件已迁移");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setMigratingStorage(false);
    }
  };

  const handleCheckUpdate = async () => {
    if (checkingUpdate || applyingUpdate || pendingUpdate || pendingVersion) return;

    try {
      const result = await runCheckUpdate();
      if (result.status === "busy") return;
      if (result.status === "latest") {
        toast.success("已是最新版本");
        return;
      }
      toast.success(`v${result.version} 已下载，点击「安装更新」完成更新`);
    } catch (error) {
      console.error("check update failed", error);
      toast.error("检查更新失败，请检查网络后重试");
    }
  };

  const handleInstallUpdate = async () => {
    if ((!pendingUpdate && !pendingVersion) || applyingUpdate || checkingUpdate) return;

    const needsRedownload = !pendingUpdate;
    toast.info(
      needsRedownload
        ? "正在确认并安装更新，安装完成后 Tempo 会重启。"
        : "正在安装更新，安装完成后 Tempo 会重启。",
      { duration: 8000 },
    );
    try {
      const result = await runInstallUpdate();
      if (result === "latest") {
        toast.success("已是最新版本");
      }
    } catch (error) {
      console.error("install update failed", error);
      toast.error("安装更新失败，请稍后重试");
    }
  };

  const updatePercent = updateProgress?.total
    ? Math.min(100, Math.round((updateProgress.downloaded / updateProgress.total) * 100))
    : updateProgress?.phase === "installing" || updateProgress?.phase === "ready"
      ? 100
      : 0;

  if (!settings) return <p className="text-sm text-muted-foreground">加载中...</p>;

  return (
    <div className="mx-auto max-w-xl space-y-6">
      <Section title="通用">
        <Card>
          <Row label="开机自启" desc="默认关闭">
            <Switch checked={settings.autostart} onCheckedChange={(v) => update({ autostart: v })} />
          </Row>
          <Row label="提醒音效">
            <Switch checked={settings.sound_enabled} onCheckedChange={(v) => update({ sound_enabled: v })} />
          </Row>
          <Row label="外观">
            <Select
              items={THEME_OPTIONS}
              value={settings.theme}
              onValueChange={(v) => v && update({ theme: v as Settings["theme"] })}
            >
              <SelectTrigger className="h-9 w-32 text-[13px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {THEME_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Row>
        </Card>
      </Section>

      <Section title="存储">
        <Card>
          <CardContent className="flex items-center gap-3 p-4">
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium">文件存储位置</p>
              <p
                className="mt-1 truncate text-[12px] text-muted-foreground"
                title={settings.storage_dir}
              >
                {settings.storage_dir || "默认位置（AppData\\Tempo）"}
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              className="shrink-0"
              disabled={migratingStorage}
              onClick={changeStorageDir}
            >
              <FolderOpen className="h-3.5 w-3.5" />
              {migratingStorage ? "迁移中" : "更换"}
            </Button>
          </CardContent>
        </Card>
      </Section>

      <Section title="MCP 服务">
        <Card>
          <Row
            label="启用 MCP 服务"
            labelExtra={<McpCapabilitiesHint />}
          >
            <Switch
              checked={settings.mcp_enabled}
              onCheckedChange={(v) => update({ mcp_enabled: v })}
            />
          </Row>
          {settings.mcp_enabled && (
            <div className="space-y-4 border-t border-border/50 px-4 py-4">
              <div>
                <Label className="text-[13px]">端口</Label>
                <div className="mt-2 flex items-center gap-2">
                  <Input
                    type="number"
                    min={1024}
                    max={65535}
                    value={mcpPortDraft}
                    onChange={(e) => setMcpPortDraft(e.target.value)}
                    onBlur={() => {
                      const port = Number(mcpPortDraft);
                      if (!Number.isFinite(port) || port < 1024 || port > 65535) {
                        setMcpPortDraft(String(settings.mcp_port));
                        toast.error("端口需在 1024–65535");
                        return;
                      }
                      if (port !== settings.mcp_port) {
                        void update({ mcp_port: port });
                      }
                    }}
                    className="h-9 w-28 border-0 glass-subtle"
                  />
                  <span className="text-[12px] text-muted-foreground">仅监听 127.0.0.1</span>
                </div>
              </div>

              <div>
                <Label className="text-[13px]">连接地址</Label>
                <div className="mt-2 flex items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded-md bg-muted/60 px-2 py-1.5 text-[12px]">
                    {`http://127.0.0.1:${settings.mcp_port}/mcp`}
                  </code>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={async () => {
                      await navigator.clipboard.writeText(
                        `http://127.0.0.1:${settings.mcp_port}/mcp`
                      );
                      toast.success("已复制 URL");
                    }}
                  >
                    <Copy className="h-3.5 w-3.5" />
                    复制
                  </Button>
                </div>
              </div>

              <div>
                <Label className="text-[13px]">访问令牌</Label>
                <div className="mt-2 flex items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded-md bg-muted/60 px-2 py-1.5 text-[12px]">
                    {showMcpToken ? settings.mcp_token : "••••••••••••••••"}
                  </code>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setShowMcpToken((v) => !v)}
                  >
                    {showMcpToken ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={async () => {
                      await navigator.clipboard.writeText(settings.mcp_token);
                      toast.success("已复制令牌");
                    }}
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={async () => {
                      try {
                        const next = await api.regenerateMcpToken();
                        setSettings(next);
                        setMcpPortDraft(String(next.mcp_port));
                        toast.success("已轮换令牌");
                      } catch (error) {
                        toast.error(error instanceof Error ? error.message : String(error));
                      }
                    }}
                  >
                    <RefreshCw className="h-3.5 w-3.5" />
                    轮换
                  </Button>
                </div>
              </div>

              <div>
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-2"
                  onClick={async () => {
                    const config = {
                      mcpServers: {
                        tempo: {
                          url: `http://127.0.0.1:${settings.mcp_port}/mcp`,
                          headers: {
                            Authorization: `Bearer ${settings.mcp_token}`,
                          },
                        },
                      },
                    };
                    await navigator.clipboard.writeText(JSON.stringify(config, null, 2));
                    toast.success("已复制");
                  }}
                >
                  <Copy className="h-3.5 w-3.5" />
                  复制配置
                </Button>
              </div>
            </div>
          )}
        </Card>
      </Section>

      <Section title="快捷键">
        <Card>
          <ShortcutRow
            label="快速添加待办"
            desc="全局唤起快速待办输入"
            value={settings.shortcut_quick_todo}
            onChange={(value) => updateShortcut("shortcut_quick_todo", value)}
          />
          <ShortcutRow
            label="剪贴板货架"
            desc="全局打开剪贴板历史"
            value={settings.shortcut_clipboard_picker}
            onChange={(value) => updateShortcut("shortcut_clipboard_picker", value)}
          />
          <ShortcutRow
            label="快捷短语货架"
            desc="全局打开快捷短语"
            value={settings.shortcut_snippet_picker}
            onChange={(value) => updateShortcut("shortcut_snippet_picker", value)}
          />
          <div className="border-t border-border/50 px-4 py-3">
            <Button
                variant="outline"
                size="sm"
                onClick={() =>
                    void update({
                      shortcut_quick_todo: DEFAULT_SHORTCUTS.shortcut_quick_todo,
                      shortcut_clipboard_picker: DEFAULT_SHORTCUTS.shortcut_clipboard_picker,
                      shortcut_snippet_picker: DEFAULT_SHORTCUTS.shortcut_snippet_picker,
                    })
                }
            >
              恢复默认
            </Button>
          </div>
        </Card>
      </Section>

      <Section title="护眼提醒">
        <Card>
          <Row label="启用">
            <Switch checked={settings.eye_care_enabled} onCheckedChange={(v) => update({ eye_care_enabled: v })} />
          </Row>
          {settings.eye_care_enabled && (
            <div className="space-y-4 border-t border-border/50 px-4 py-4">
              <Label className="text-[13px]">周期 · {settings.eye_care_interval_minutes} 分钟</Label>
              <Slider className="mt-3" min={15} max={90} step={5}
                value={[settings.eye_care_interval_minutes]}
                onValueChange={([v]) => update({ eye_care_interval_minutes: v })} />
              <Button
                variant="outline"
                size="sm"
                onClick={() => emit("reminder", { type: "eye_care" })}
              >
                测试全屏提醒
              </Button>
            </div>
          )}
        </Card>
      </Section>

      <Section title="夜间提醒">
        <Card>
          <Row label="启用">
            <Switch checked={settings.night_reminder_enabled} onCheckedChange={(v) => update({ night_reminder_enabled: v })} />
          </Row>
          {settings.night_reminder_enabled && (
            <div className="flex gap-4 border-t border-border/50 px-4 py-4">
              <div>
                <Label className="text-[11px] text-muted-foreground">开始</Label>
                <Input type="time" value={settings.night_reminder_start}
                  onChange={(e) => update({ night_reminder_start: e.target.value })}
                  className="mt-1 h-9 w-28 border-0 glass-subtle" />
              </div>
              <div>
                <Label className="text-[11px] text-muted-foreground">结束</Label>
                <Input type="time" value={settings.night_reminder_end}
                  onChange={(e) => update({ night_reminder_end: e.target.value })}
                  className="mt-1 h-9 w-28 border-0 glass-subtle" />
              </div>
            </div>
          )}
        </Card>
      </Section>

      <Section title="剪贴板">
        <Card>
          <Row label="记录剪贴板" desc="自动保存复制过的文字与截图">
            <Switch
              checked={settings.clipboard_monitor_enabled}
              onCheckedChange={(v) => update({ clipboard_monitor_enabled: v })}
            />
          </Row>

          <div className="space-y-4 border-t border-border/50 px-4 py-4">
            <div>
              <p className="text-[14px] font-medium">粘贴项目</p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">从剪贴板历史选择项目时的行为</p>
              <div className="mt-3 space-y-2">
                <PasteModeOption
                  selected={settings.clipboard_paste_mode === "active_app"}
                  title="到当前活动应用"
                  description="将选定的项目直接粘贴到您当前正在使用的应用程序中。"
                  onSelect={() => update({ clipboard_paste_mode: "active_app" })}
                />
                <PasteModeOption
                  selected={settings.clipboard_paste_mode === "clipboard"}
                  title="到剪贴板"
                  description="将选定的项目复制到系统剪贴板，以便稍后手动粘贴。"
                  onSelect={() => update({ clipboard_paste_mode: "clipboard" })}
                />
              </div>
            </div>

            <Row label="始终以纯文本粘贴" desc="忽略富文本格式，仅粘贴纯文本内容">
              <Switch
                checked={settings.clipboard_plain_text_only}
                onCheckedChange={(v) => update({ clipboard_plain_text_only: v })}
              />
            </Row>
          </div>

          <div className="space-y-4 border-t border-border/50 px-4 py-4">
            <div>
              <p className="text-[14px] font-medium">保留历史</p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                {clipboardRetentionLabel(settings.clipboard_history_retention)}
              </p>
              <div className="mt-4">
                <Slider
                  min={0}
                  max={4}
                  step={1}
                  value={[clipboardRetentionIndex(settings.clipboard_history_retention)]}
                  onValueChange={([value]) =>
                    update({ clipboard_history_retention: clipboardRetentionValue(value) })
                  }
                />
                <div className="mt-2 flex justify-between text-[11px] text-muted-foreground">
                  {CLIPBOARD_RETENTION_OPTIONS.map((option) => (
                    <span key={option.value}>{option.label}</span>
                  ))}
                </div>
              </div>
            </div>
            <div className="flex justify-end">
              <Button
                variant="outline"
                size="sm"
                onClick={async () => {
                  if (!confirm("确定清空全部未固定的剪贴板历史？")) return;
                  const count = await api.clearClipboardHistory();
                  toast.success(count > 0 ? `已清空 ${count} 条记录` : "没有可清空的记录");
                }}
              >
                清空历史
              </Button>
            </div>
          </div>
        </Card>
      </Section>

      <Section title="关于">
        <Card>
          <CardContent className="space-y-3 p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-[14px] font-medium">Tempo</p>
                <p className="mt-1 text-[12px] text-muted-foreground">
                  当前版本 {appVersion || "..."}
                  {pendingVersion
                    ? pendingUpdate
                      ? ` · 已下载 v${pendingVersion}`
                      : ` · 待安装 v${pendingVersion}`
                    : ""}
                </p>
              </div>
              {pendingVersion ? (
                <Button
                  size="sm"
                  className="shrink-0"
                  disabled={applyingUpdate || checkingUpdate}
                  onClick={() => void handleInstallUpdate()}
                >
                  <RotateCcw
                    className={`h-3.5 w-3.5 ${applyingUpdate || checkingUpdate ? "animate-spin" : ""}`}
                  />
                  {applyingUpdate
                    ? updateProgress?.phase === "downloading"
                      ? "下载中"
                      : updateProgress?.phase === "checking"
                        ? "确认中"
                        : "安装中"
                    : "安装更新"}
                </Button>
              ) : (
                <Button
                  variant="outline"
                  size="sm"
                  className="shrink-0"
                  disabled={checkingUpdate}
                  onClick={handleCheckUpdate}
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${checkingUpdate ? "animate-spin" : ""}`} />
                  {checkingUpdate
                    ? updateProgress?.phase === "downloading"
                      ? "下载中"
                      : "检查中"
                    : "检查更新"}
                </Button>
              )}
            </div>
            {updateProgress && updateProgress.phase === "downloading" && (
              <div className="space-y-2">
                <p className="text-[12px] text-muted-foreground">
                  正在下载 {updateProgress.version ? `v${updateProgress.version}` : "更新"}...
                </p>
                <Progress value={updatePercent} className="h-1.5" />
              </div>
            )}
            {(updateProgress?.phase === "installing" || updateProgress?.phase === "done") && (
              <p className="text-[12px] text-muted-foreground">正在安装更新，完成后 Tempo 会重启...</p>
            )}
          </CardContent>
        </Card>
      </Section>

    </div>
  );
}

function McpCapabilitiesHint() {
  return (
    <Popover>
      <PopoverTrigger asChild openOnHover delay={200} closeDelay={100}>
        <button
          type="button"
          className="inline-flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          aria-label="查看 MCP 功能"
        >
          <CircleHelp className="size-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" side="bottom" initialFocus={false} className="w-72 p-3">
        <p className="mb-2 text-[13px] font-medium">MCP 可提供的能力</p>
        <ul className="space-y-1.5 text-[12px] leading-relaxed text-muted-foreground">
          <li>待办：创建、查询、更新、完成、置顶、删除、子任务与备注</li>
          <li>快捷短语：查询、创建、更新、删除、分组、复制到剪贴板</li>
          <li>剪贴板：搜索历史记录</li>
          <li>番茄钟：查看状态、开始 / 暂停 / 停止 / 跳过</li>
          <li>报告：读取今日屏幕使用报告</li>
        </ul>
      </PopoverContent>
    </Popover>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <p className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">{title}</p>
      {children}
    </div>
  );
}

function Row({
  label,
  desc,
  labelExtra,
  children,
}: {
  label: string;
  desc?: string;
  labelExtra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="list-row">
      <div>
        <p className="inline-flex items-center gap-1.5 text-[14px] font-medium">
          {label}
          {labelExtra}
        </p>
        {desc && <p className="text-[11px] text-muted-foreground">{desc}</p>}
      </div>
      {children}
    </div>
  );
}

function ShortcutRow({
  label,
  desc,
  value,
  onChange,
}: {
  label: string;
  desc?: string;
  value: string;
  onChange: (value: string) => Promise<void>;
}) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecording(false);
        return;
      }

      const next = shortcutFromKeyboardEvent(event);
      if (!next) return;

      setRecording(false);
      void onChange(next).catch(() => undefined);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [recording, onChange]);

  return (
    <div className="list-row">
      <div>
        <p className="text-[14px] font-medium">{label}</p>
        {desc && <p className="text-[11px] text-muted-foreground">{desc}</p>}
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant={recording ? "default" : "outline"}
          size="sm"
          className="min-w-28 font-mono text-[12px]"
          onClick={() => setRecording((prev) => !prev)}
        >
          {recording ? "按下快捷键" : value ? formatShortcutLabel(value) : "未设置"}
        </Button>
      </div>
    </div>
  );
}

function PasteModeOption({
  selected,
  title,
  description,
  onSelect,
}: {
  selected: boolean;
  title: string;
  description: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full items-start gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors ${
        selected
          ? "border-primary/40 bg-primary/5"
          : "border-border/60 bg-transparent hover:bg-foreground/[0.03]"
      }`}
    >
      <span
        className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${
          selected ? "border-primary" : "border-muted-foreground/40"
        }`}
      >
        {selected && <span className="h-2 w-2 rounded-full bg-primary" />}
      </span>
      <span className="min-w-0">
        <span className="block text-[13px] font-medium">{title}</span>
        <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground">
          {description}
        </span>
      </span>
    </button>
  );
}
