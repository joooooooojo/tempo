import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { emit } from "@tauri-apps/api/event";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/api";
import { formatDurationShort } from "@/lib/utils";
import type { AppLimit, AppUsage, Settings } from "@/types";

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [blocked, setBlocked] = useState<string[]>([]);
  const [limits, setLimits] = useState<AppLimit[]>([]);
  const [knownApps, setKnownApps] = useState<AppUsage[]>([]);
  const [limitApp, setLimitApp] = useState("");
  const [limitHours, setLimitHours] = useState("2");

  const load = async () => {
    const [s, b, l, a] = await Promise.all([
      api.getSettings(), api.getBlockedApps(), api.getAppLimits(), api.getKnownApps(),
    ]);
    setSettings(s); setBlocked(b); setLimits(l); setKnownApps(a);
  };

  useEffect(() => {
    load().catch(console.error);
    const timer = setInterval(() => api.getAppLimits().then(setLimits), 10000);
    return () => clearInterval(timer);
  }, []);

  const update = async (patch: Partial<Settings>) => {
    if (!settings) return;
    setSettings({ ...settings, ...patch });
    await api.updateSettings(patch);
    applyTheme(patch.theme ?? settings.theme);
    toast.success("已保存");
  };

  const applyTheme = (theme: Settings["theme"]) => {
    const root = document.documentElement;
    if (theme === "dark") root.classList.add("dark");
    else if (theme === "light") root.classList.remove("dark");
    else root.classList.toggle("dark", window.matchMedia("(prefers-color-scheme: dark)").matches);
  };

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

      <Section title="番茄时钟">
        <Card className="overflow-hidden">
          <div className="space-y-4 px-4 py-4">
            <div>
              <Label className="text-[13px]">专注时长 · {settings.pomodoro_work_minutes} 分钟</Label>
              <Slider className="mt-3" min={5} max={60} step={5}
                value={[settings.pomodoro_work_minutes]}
                onValueChange={([v]) => update({ pomodoro_work_minutes: v })} />
            </div>
            <div>
              <Label className="text-[13px]">短休时长 · {settings.pomodoro_short_break_minutes} 分钟</Label>
              <Slider className="mt-3" min={1} max={15} step={1}
                value={[settings.pomodoro_short_break_minutes]}
                onValueChange={([v]) => update({ pomodoro_short_break_minutes: v })} />
            </div>
            <div>
              <Label className="text-[13px]">长休时长 · {settings.pomodoro_long_break_minutes} 分钟</Label>
              <Slider className="mt-3" min={5} max={30} step={5}
                value={[settings.pomodoro_long_break_minutes]}
                onValueChange={([v]) => update({ pomodoro_long_break_minutes: v })} />
            </div>
            <div>
              <Label className="text-[13px]">长休间隔 · 每 {settings.pomodoro_sessions_per_cycle} 轮专注</Label>
              <Slider className="mt-3" min={2} max={8} step={1}
                value={[settings.pomodoro_sessions_per_cycle]}
                onValueChange={([v]) => update({ pomodoro_sessions_per_cycle: v })} />
            </div>
          </div>
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

      <Section title="应用限额">
        <Card>
          <CardContent className="space-y-3 p-4">
            <div className="flex gap-2">
              <Select value={limitApp} onValueChange={setLimitApp}>
                <SelectTrigger className="flex-1 glass-subtle border-0"><SelectValue placeholder="选择应用" /></SelectTrigger>
                <SelectContent>{knownApps.map((a) => <SelectItem key={a.app_name} value={a.app_name}>{a.app_name}</SelectItem>)}</SelectContent>
              </Select>
              <Input type="number" min={0.5} step={0.5} value={limitHours} onChange={(e) => setLimitHours(e.target.value)}
                className="w-16 border-0 glass-subtle" placeholder="h" />
              <Button size="sm" onClick={async () => {
                if (!limitApp) return;
                await api.setAppLimit(limitApp, Math.round(parseFloat(limitHours) * 3600));
                setLimitApp(""); load(); toast.success("已设置");
              }}>添加</Button>
            </div>
            {limits.map((l) => (
              <div key={l.app_name} className="glass-subtle rounded-lg p-3">
                <div className="flex items-start justify-between gap-3 text-[13px]">
                  <span className="min-w-0 truncate font-semibold">{l.app_name}</span>
                  <span className="stat-value shrink-0 text-muted-foreground">{formatDurationShort(l.used_seconds)} / {formatDurationShort(l.limit_seconds)}</span>
                </div>
                <Progress value={(l.used_seconds / l.limit_seconds) * 100} className="mt-2 h-1" />
                <Button variant="ghost" size="sm" className="mt-1 h-6 text-[11px] text-rose-600 hover:bg-rose-500/10 dark:text-rose-300"
                  onClick={async () => { await api.removeAppLimit(l.app_name); load(); }}>移除</Button>
              </div>
            ))}
          </CardContent>
        </Card>
      </Section>

      <Section title="应用屏蔽">
        <Card className="overflow-hidden">
          {knownApps.slice(0, 15).map((a) => (
            <Row key={a.app_name} label={a.app_name}>
              <Switch checked={blocked.includes(a.app_name)} onCheckedChange={async (v) => {
                if (v) await api.blockApp(a.app_name); else await api.unblockApp(a.app_name); load();
              }} />
            </Row>
          ))}
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
