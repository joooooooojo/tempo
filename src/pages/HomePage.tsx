import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { TrendingUp, Calendar, Zap, type LucideIcon } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { AppIcon } from "@/components/AppIcon";
import { FlipNumber } from "@/components/FlipNumber";
import { api } from "@/lib/api";
import { cn, formatDuration, formatDurationShort, getDurationParts } from "@/lib/utils";
import type { DashboardData } from "@/types";

const GOAL_SECONDS = 8 * 3600;

const fallbackGradients = [
  "from-emerald-400 to-green-600",
  "from-emerald-400 to-teal-600",
  "from-amber-400 to-orange-500",
  "from-lime-300 to-emerald-500",
  "from-slate-400 to-slate-600",
];

export function HomePage() {
  const [data, setData] = useState<DashboardData | null>(null);

  useEffect(() => {
    let cancelled = false;
    let pending = false;
    const load = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await api.getDashboard();
        if (!cancelled) setData(next);
      } catch (error) {
        console.error(error);
      } finally {
        pending = false;
      }
    };

    void load();
    const unlisten = listen<DashboardData>("dashboard-update", (e) => {
      if (!cancelled) setData(e.payload);
    });
    const fallback = window.setInterval(() => void load(), 1000);
    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
      window.clearInterval(fallback);
    };
  }, []);

  if (!data) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="glass rounded-lg px-8 py-5 text-sm text-muted-foreground">
          加载数据中...
        </div>
      </div>
    );
  }

  const hasData = data.today_screen_seconds > 0 || data.top_apps.length > 0;
  const maxAppSeconds = data.top_apps[0]?.seconds ?? 1;
  const pct = Math.min((data.today_screen_seconds / GOAL_SECONDS) * 100, 100);

  return (
    <div className="space-y-5">
      {/* Bento grid */}
      <div className="grid grid-cols-12 items-start gap-3">
        {/* Hero stat — spans 7 cols */}
        <Card className="glass-glow col-span-7 overflow-hidden">
          <CardContent className="relative p-6">
            <div className="flex items-start justify-between">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                  今日亮屏
                </p>
                <DurationSegments seconds={data.today_screen_seconds} variant="hero" />
                <p className="mt-3 text-[13px] text-muted-foreground">{data.status_message}</p>
              </div>
              <div className="glass-subtle flex h-24 w-24 shrink-0 flex-col justify-between rounded-xl p-3">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                  目标
                </span>
                <span className="stat-value text-2xl font-extrabold text-primary">
                  {Math.round(pct)}%
                </span>
                <div className="progress-track h-1.5 rounded-sm bg-foreground/8">
                  <div
                    className="progress-fill h-full rounded-sm bg-gradient-to-r from-emerald-300 to-teal-400 transition-all duration-700"
                    style={{ width: `${pct}%` }}
                  />
                </div>
              </div>
            </div>
            <div className="mt-5 flex items-center gap-2 glass-subtle rounded-lg px-3 py-2">
              <Zap className="activity-spark h-3.5 w-3.5 text-amber-400" />
              <span className="text-[12px] text-muted-foreground">
                连续使用 {formatDuration(data.continuous_screen_seconds)}
              </span>
            </div>
          </CardContent>
        </Card>

        {/* Side stats — spans 5 cols */}
        <div className="col-span-5 flex flex-col gap-2.5">
          <StatCard
            icon={TrendingUp}
            label="近 7 日"
            seconds={data.week_screen_seconds}
          />
          <StatCard
            icon={Calendar}
            label="近 30 日"
            seconds={data.month_screen_seconds}
          />
        </div>
      </div>

      {/* App ranking */}
      <div>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">应用排行</h2>
          <span className="text-[11px] text-muted-foreground">TOP 5 · 今日</span>
        </div>

        <Card className="overflow-hidden bg-card/70">
          {!hasData || data.top_apps.length === 0 ? (
            <CardContent className="flex flex-col items-center py-14">
              <div className="flex h-12 w-12 items-center justify-center rounded-lg bg-foreground/5">
                <Zap className="h-5 w-5 text-muted-foreground" />
              </div>
              <p className="mt-3 text-sm font-medium">暂无使用数据</p>
              <p className="mt-1 text-[12px] text-muted-foreground">开始使用电脑后将自动统计</p>
            </CardContent>
          ) : (
            <div className="divide-y divide-border/45">
              {data.top_apps.slice(0, 5).map((app, i) => {
                const grad = fallbackGradients[i % fallbackGradients.length];
                const percent = Math.round((app.seconds / maxAppSeconds) * 100);
                return (
                  <div
                    key={app.app_name}
                    className={cn(
                      "rank-row grid grid-cols-[34px_48px_minmax(0,1fr)_104px] items-center gap-3 px-5 py-3.5 transition-colors hover:bg-foreground/[0.03]",
                      i === 0 && "bg-primary/[0.035]"
                    )}
                    style={{ animationDelay: `${i * 45}ms` }}
                  >
                    <span
                      className={cn(
                        "rank-badge flex h-7 w-7 items-center justify-center rounded-md text-[11px] font-bold",
                        i === 0
                          ? "bg-primary text-primary-foreground shadow-md shadow-primary/20"
                          : "bg-foreground/5 text-muted-foreground"
                      )}
                    >
                      {String(i + 1).padStart(2, "0")}
                    </span>
                    <AppIcon
                      name={app.app_name}
                      iconDataUrl={app.icon_data_url}
                      className="rank-icon"
                      fallbackClassName={`bg-gradient-to-br ${grad}`}
                    />
                    <div className="min-w-0">
                      <p className="truncate text-[14px] font-semibold">{app.app_name}</p>
                      <div className="mt-2 flex items-center gap-2">
                        <div className="progress-track h-1.5 min-w-0 flex-1 rounded-sm bg-foreground/8">
                          <div
                            className="progress-fill h-full rounded-sm bg-gradient-to-r from-emerald-300 via-teal-300 to-lime-400 transition-all duration-500"
                            style={{ width: `${percent}%` }}
                          />
                        </div>
                        <span className="w-8 text-right text-[10px] font-semibold text-muted-foreground">
                          {percent}%
                        </span>
                      </div>
                    </div>
                    <span className="time-pill stat-value justify-self-end rounded-md bg-primary/10 px-2.5 py-1 text-[13px] font-bold text-primary">
                      {formatDurationShort(app.seconds)}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}

function StatCard({
  icon: Icon,
  label,
  seconds,
}: {
  icon: LucideIcon;
  label: string;
  seconds: number;
}) {
  return (
    <Card>
      <CardContent className="p-3.5">
        <div className="flex items-center justify-between">
          <p className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {label}
          </p>
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-primary/10">
            <Icon className="h-3.5 w-3.5 text-primary" strokeWidth={2} />
          </div>
        </div>
        <div className="mt-2.5">
          <DurationSegments seconds={seconds} variant="card" />
        </div>
      </CardContent>
    </Card>
  );
}

function DurationSegments({
  seconds,
  variant,
}: {
  seconds: number;
  variant: "hero" | "card";
}) {
  const parts = getDurationParts(seconds);

  return (
    <div
      className={cn(
        "flex flex-wrap items-end",
        variant === "hero" ? "mt-3 gap-x-3 gap-y-2" : "gap-x-2.5 gap-y-1"
      )}
    >
      {parts.map((part, index) => (
        <span key={part.unit} className="inline-flex items-end gap-1.5">
          <FlipNumber
            value={part.value}
            size={variant === "hero" ? "hero" : "compact"}
            tone={index === 0 ? "primary" : "muted"}
          />
          <span
            className={cn(
              "font-semibold text-emerald-900/45 dark:text-emerald-50/45",
              variant === "hero" ? "mb-1 text-[14px]" : "mb-0.5 text-[11px]"
            )}
          >
            {part.unit}
          </span>
        </span>
      ))}
    </div>
  );
}
