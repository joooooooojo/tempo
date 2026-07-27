import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api } from "@/lib/api";
import type { Settings } from "@/types";
import {
  CLIPBOARD_RETENTION_OPTIONS,
  clipboardRetentionIndex,
  clipboardRetentionLabel,
  clipboardRetentionValue,
  PasteModeOption,
  Row,
  Section,
} from "@/pages/settings/shared";
import type { BuiltinConfigPanelProps } from "@/pages/settings/builtins/types";

export function ClipboardBuiltinConfig({ busy, onBusyChange }: BuiltinConfigPanelProps) {
  const [settings, setSettings] = useState<Settings | null>(null);

  const load = useCallback(async () => {
    try {
      setSettings(await api.getSettings());
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const update = async (patch: Partial<Settings>) => {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, ...patch });
    // Do not toggle parent busy here — it re-renders the plugin list and used to
    // remount this dialog (unstable load deps). Keep busy for destructive actions only.
    try {
      await api.updateSettings(patch);
    } catch (error) {
      setSettings(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  if (!settings) {
    return (
      <p className="py-6 text-center text-[13px] text-muted-foreground">加载中…</p>
    );
  }

  return (
    <div className="settings-panel-stack">
      <Section title="行为">
        <Card>
          <Row label="记录剪贴板" desc="自动保存复制过的文字与截图">
            <Switch
              checked={settings.clipboard_monitor_enabled}
              onCheckedChange={(value) => void update({ clipboard_monitor_enabled: value })}
            />
          </Row>

          <div className="space-y-4 border-t border-border/50 px-4 py-4">
            <div>
              <p className="text-[14px] font-medium">粘贴项目</p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                从剪贴板历史选择项目时的行为
              </p>
              <div className="mt-3 space-y-2">
                <PasteModeOption
                  selected={settings.clipboard_paste_mode === "active_app"}
                  title="到当前活动应用"
                  description="将选定的项目直接粘贴到您当前正在使用的应用程序中。"
                  onSelect={() => {
                    void update({ clipboard_paste_mode: "active_app" });
                  }}
                />
                <PasteModeOption
                  selected={settings.clipboard_paste_mode === "clipboard"}
                  title="到剪贴板"
                  description="将选定的项目复制到系统剪贴板，以便稍后手动粘贴。"
                  onSelect={() => {
                    void update({ clipboard_paste_mode: "clipboard" });
                  }}
                />
              </div>
            </div>

            <Row label="始终以纯文本粘贴" desc="忽略富文本格式，仅粘贴纯文本内容">
              <Switch
                checked={settings.clipboard_plain_text_only}
                onCheckedChange={(value) => void update({ clipboard_plain_text_only: value })}
              />
            </Row>
          </div>
        </Card>
      </Section>

      <Section title="历史">
        <Card>
          <div className="space-y-4 px-4 py-4">
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
                  disabled={busy}
                  value={[clipboardRetentionIndex(settings.clipboard_history_retention)]}
                  onValueChange={([value]) =>
                    void update({
                      clipboard_history_retention: clipboardRetentionValue(value),
                    })
                  }
                />
                <div className="mt-2 flex justify-between text-[11px] text-muted-foreground">
                  {CLIPBOARD_RETENTION_OPTIONS.map((option) => (
                    <span key={option.value}>{option.label}</span>
                  ))}
                </div>
              </div>
            </div>
            <div className="flex justify-end border-t border-border/50 pt-3">
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={async () => {
                  if (!confirm("确定清空全部未固定的剪贴板历史？")) return;
                  onBusyChange(true);
                  try {
                    const count = await api.clearClipboardHistory();
                    toast.success(count > 0 ? `已清空 ${count} 条记录` : "没有可清空的记录");
                  } catch (error) {
                    toast.error(error instanceof Error ? error.message : String(error));
                  } finally {
                    onBusyChange(false);
                  }
                }}
              >
                清空历史
              </Button>
            </div>
          </div>
        </Card>
      </Section>
    </div>
  );
}
