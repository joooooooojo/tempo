import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { emit } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Update } from "@tauri-apps/plugin-updater";
import { FolderOpen, RefreshCw, RotateCcw } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/api";
import { emitThemeChange } from "@/lib/theme";
import {
  checkAndDownloadUpdate,
  getAppVersion,
  installAndRelaunch,
  type UpdateProgress,
} from "@/lib/update";
import type { Settings } from "@/types";

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [migratingStorage, setMigratingStorage] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [applyingUpdate, setApplyingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [pendingVersion, setPendingVersion] = useState("");

  const load = async () => {
    const s = await api.getSettings();
    setSettings(s);
  };

  useEffect(() => {
    load().catch(console.error);
    getAppVersion().then(setAppVersion).catch(console.error);
  }, []);

  const update = async (patch: Partial<Settings>) => {
    if (!settings) return;
    setSettings({ ...settings, ...patch });
    await api.updateSettings(patch);
    if (patch.theme !== undefined) {
      await emitThemeChange(patch.theme);
    }
    toast.success("已保存");
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
    if (checkingUpdate || applyingUpdate || pendingUpdate) return;

    setCheckingUpdate(true);
    setUpdateProgress({ phase: "checking", downloaded: 0, total: 0 });

    try {
      const result = await checkAndDownloadUpdate(setUpdateProgress);
      if (result.status === "latest") {
        toast.success("已是最新版本");
        setUpdateProgress(null);
        return;
      }

      setPendingUpdate(result.update);
      setPendingVersion(result.version);
      setUpdateProgress({
        phase: "ready",
        downloaded: 0,
        total: 0,
        version: result.version,
      });
      toast.success(`v${result.version} 已下载，点击「重启更新」完成安装`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      setUpdateProgress(null);
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleRestartUpdate = async () => {
    if (!pendingUpdate || applyingUpdate) return;

    setApplyingUpdate(true);
    try {
      await installAndRelaunch(pendingUpdate, setUpdateProgress);
    } catch (error) {
      setApplyingUpdate(false);
      toast.error(error instanceof Error ? error.message : String(error));
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
        <Card className="overflow-hidden">
          <Row label="开机自启" desc="默认关闭">
            <Switch checked={settings.autostart} onCheckedChange={(v) => update({ autostart: v })} />
          </Row>
          <Row label="提醒音效">
            <Switch checked={settings.sound_enabled} onCheckedChange={(v) => update({ sound_enabled: v })} />
          </Row>
          <Row label="外观">
            <Select value={settings.theme} onValueChange={(v) => update({ theme: v as Settings["theme"] })}>
              <SelectTrigger className="h-8 w-28 border-0 bg-transparent text-[13px] shadow-none">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="system">跟随系统</SelectItem>
                <SelectItem value="light">浅色</SelectItem>
                <SelectItem value="dark">深色</SelectItem>
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

      <Section title="护眼提醒">
        <Card className="overflow-hidden">
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
        <Card className="overflow-hidden">
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

      <Section title="关于">
        <Card>
          <CardContent className="space-y-3 p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-[14px] font-medium">Tempo</p>
                <p className="mt-1 text-[12px] text-muted-foreground">
                  当前版本 {appVersion || "..."}
                  {pendingVersion ? ` · 已下载 v${pendingVersion}` : ""}
                </p>
              </div>
              {pendingUpdate ? (
                <Button
                  size="sm"
                  className="shrink-0"
                  disabled={applyingUpdate}
                  onClick={() => void handleRestartUpdate()}
                >
                  <RotateCcw className={`h-3.5 w-3.5 ${applyingUpdate ? "animate-spin" : ""}`} />
                  {applyingUpdate ? "安装中" : "重启更新"}
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
            {updateProgress?.phase === "installing" && (
              <p className="text-[12px] text-muted-foreground">正在静默安装并重启...</p>
            )}
          </CardContent>
        </Card>
      </Section>

      <Section title="数据">
        <Card>
          <CardContent className="flex gap-2 p-4">
            <Button variant="outline" size="sm" className="flex-1" onClick={async () => { await api.resetToday(); toast.success("已重置"); }}>重置今日</Button>
            <Button variant="destructive" size="sm" className="flex-1" onClick={async () => {
              if (confirm("确定清空全部历史？")) { await api.resetAll(); toast.success("已清空"); }
            }}>清空全部</Button>
          </CardContent>
        </Card>
      </Section>
    </div>
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

function Row({ label, desc, children }: { label: string; desc?: string; children: ReactNode }) {
  return (
    <div className="list-row">
      <div>
        <p className="text-[14px] font-medium">{label}</p>
        {desc && <p className="text-[11px] text-muted-foreground">{desc}</p>}
      </div>
      {children}
    </div>
  );
}
