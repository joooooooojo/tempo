import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Copy, Eye, EyeOff, RefreshCw, RotateCcw } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/api";
import type { Settings } from "@/types";
import {
  DEFAULT_SHORTCUTS,
  McpCapabilitiesHint,
  Row,
  Section,
  ShortcutRow,
  SHORTCUT_SETTING_KEYS,
  THEME_OPTIONS,
  type SettingsUpdater,
  type ShortcutSettingKey,
  normalizeShortcutForComparison,
} from "@/builtin-plugins/settings/pages/shared";
import { PluginRuntimeSection } from "@/builtin-plugins/settings/pages/PluginRuntimeSection";

interface GeneralSettingsPanelProps {
  settings: Settings;
  update: SettingsUpdater;
  onSettingsChange: (settings: Settings) => void;
  appVersion: string;
  checkingUpdate: boolean;
  applyingUpdate: boolean;
  updatePercent: number;
  pendingUpdate: boolean;
  pendingVersion: string | null;
  updatePhase?: string | null;
  updateDownloadVersion?: string | null;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
}

export function GeneralSettingsPanel({
  settings,
  update,
  onSettingsChange,
  appVersion,
  checkingUpdate,
  applyingUpdate,
  updatePercent,
  pendingUpdate,
  pendingVersion,
  updatePhase,
  updateDownloadVersion,
  onCheckUpdate,
  onInstallUpdate,
}: GeneralSettingsPanelProps) {
  const [showMcpToken, setShowMcpToken] = useState(false);
  const [mcpPortDraft, setMcpPortDraft] = useState(String(settings.mcp_port));

  useEffect(() => {
    setMcpPortDraft(String(settings.mcp_port));
  }, [settings.mcp_port]);

  const updateShortcut = (key: ShortcutSettingKey, value: string) => {
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

  return (
    <div className="settings-panel-stack">
      <Section title="基础">
        <Card>
          <Row label="开机自启" desc="登录系统后自动启动 Tempo">
            <Switch
              checked={settings.autostart}
              onCheckedChange={(value) => void update({ autostart: value })}
            />
          </Row>
          <Row label="提醒音效" desc="待办等提醒播放提示音">
            <Switch
              checked={settings.sound_enabled}
              onCheckedChange={(value) => void update({ sound_enabled: value })}
            />
          </Row>
          <Row label="外观" desc="界面配色跟随系统或手动指定">
            <Select
              items={THEME_OPTIONS}
              value={settings.theme}
              onValueChange={(value) => value && void update({ theme: value as Settings["theme"] })}
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

      <PluginRuntimeSection />

      <Section title="快捷键">
        <Card>
          <ShortcutRow
            label="主面板"
            desc="全局搜索应用并执行快捷操作"
            value={settings.shortcut_main_panel}
            onChange={(value) => updateShortcut("shortcut_main_panel", value)}
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
                  shortcut_main_panel: DEFAULT_SHORTCUTS.shortcut_main_panel,
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

      <Section title="MCP 服务">
        <Card>
          <Row
            label="启用 MCP 服务"
            labelExtra={<McpCapabilitiesHint />}
            desc="供 Cursor 等客户端调用 Tempo 能力"
          >
            <Switch
              checked={settings.mcp_enabled}
              onCheckedChange={(value) => void update({ mcp_enabled: value })}
            />
          </Row>
          {settings.mcp_enabled ? (
            <div className="space-y-4 border-t border-border/50 px-4 py-4">
              <div>
                <Label className="text-[13px]">端口</Label>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Input
                    type="number"
                    min={1024}
                    max={65535}
                    value={mcpPortDraft}
                    onChange={(event) => setMcpPortDraft(event.target.value)}
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
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded-md bg-muted/60 px-2 py-1.5 text-[12px]">
                    {showMcpToken ? settings.mcp_token : "••••••••••••••••"}
                  </code>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setShowMcpToken((value) => !value)}
                  >
                    {showMcpToken ? (
                      <EyeOff className="h-3.5 w-3.5" />
                    ) : (
                      <Eye className="h-3.5 w-3.5" />
                    )}
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
                        onSettingsChange(next);
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
          ) : null}
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
                  onClick={() => void onInstallUpdate()}
                >
                  <RotateCcw
                    className={`h-3.5 w-3.5 ${applyingUpdate || checkingUpdate ? "animate-spin" : ""}`}
                  />
                  {applyingUpdate
                    ? updatePhase === "downloading"
                      ? "下载中"
                      : updatePhase === "checking"
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
                  onClick={() => void onCheckUpdate()}
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${checkingUpdate ? "animate-spin" : ""}`} />
                  {checkingUpdate
                    ? updatePhase === "downloading"
                      ? "下载中"
                      : "检查中"
                    : "检查更新"}
                </Button>
              )}
            </div>
            {updatePhase === "downloading" ? (
              <div className="space-y-2">
                <p className="text-[12px] text-muted-foreground">
                  正在下载 {updateDownloadVersion ? `v${updateDownloadVersion}` : "更新"}...
                </p>
                <Progress value={updatePercent} className="h-1.5" />
              </div>
            ) : null}
            {updatePhase === "installing" || updatePhase === "done" ? (
              <p className="text-[12px] text-muted-foreground">
                正在安装更新，完成后 Tempo 会重启...
              </p>
            ) : null}
          </CardContent>
        </Card>
      </Section>
    </div>
  );
}
