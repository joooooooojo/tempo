import { useEffect, useState } from "react";
import {
  BarChart, Bar, Cell, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Download } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { AppIcon } from "@/components/AppIcon";
import { api } from "@/lib/api";
import { formatDuration, formatDurationShort } from "@/lib/utils";
import type { DailyReport, WeeklyReport } from "@/types";

const ACCENT_DEEP = "#10b981";
const AXIS = "#6b7f78";
const GRID = "rgba(42, 84, 70, 0.1)";

export function ReportsPage() {
  const [daily, setDaily] = useState<DailyReport | null>(null);
  const [weekly, setWeekly] = useState<WeeklyReport | null>(null);

  useEffect(() => {
    api.getDailyReport().then(setDaily).catch(console.error);
    api.getWeeklyReport().then(setWeekly).catch(console.error);
  }, []);

  const handleExport = async () => {
    const path = await save({
      filters: [{ name: "CSV", extensions: ["csv"] }],
      defaultPath: `时窗报表_${new Date().toISOString().slice(0, 10)}.csv`,
    });
    if (path) await api.exportReport(path);
  };

  const hourlyChart = daily?.hourly.map((h) => ({
    hour: h.hour,
    label: `${String(h.hour).padStart(2, "0")}:00`,
    seconds: h.seconds,
    isPeak: h.seconds > 0 && h.hour === daily.peak_hour,
  })) ?? [];
  const weeklyChart = weekly?.days.map((d) => ({
    date: d.date.slice(5),
    label: d.date.slice(5),
    seconds: d.seconds,
    isOverLimit: d.is_over_limit,
  })) ?? [];
  const weeklyPeak = weeklyChart.reduce(
    (peak, day) => (day.seconds > peak.seconds ? day : peak),
    { date: "", label: "", seconds: 0, isOverLimit: false }
  );
  const weeklyAxis = getWeeklyAxis(weeklyChart.map((day) => day.seconds));

  return (
    <div>
      <Tabs defaultValue="daily">
        <div className="mb-4 flex items-center justify-between gap-3">
          <TabsList className="w-fit">
            <TabsTrigger value="daily">日报</TabsTrigger>
            <TabsTrigger value="weekly">周报</TabsTrigger>
          </TabsList>

          <Button variant="outline" size="sm" className="h-9 rounded-lg px-4" onClick={handleExport}>
            <Download className="mr-1.5 h-3.5 w-3.5" />
            导出 CSV
          </Button>
        </div>

        <TabsContent value="daily">
          {!daily ? <EmptyState /> : (
            <div className="space-y-4">
              <div className="grid grid-cols-4 gap-3">
                <MiniStat label="总时长" value={formatDuration(daily.total_seconds)} highlight />
                <MiniStat label="均/小时" value={formatDuration(daily.average_seconds)} />
                <MiniStat label="峰值时段" value={`${daily.peak_hour}:00`} />
                <MiniStat label="高频应用" value={daily.top_apps[0]?.app_name ?? "—"} />
              </div>

              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-0">
                  <CardTitle>每小时趋势</CardTitle>
                  {daily.peak_seconds > 0 && (
                    <span className="rounded-md bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary">
                      峰值 {String(daily.peak_hour).padStart(2, "0")}:00 · {formatDurationShort(daily.peak_seconds)}
                    </span>
                  )}
                </CardHeader>
                <CardContent className="pt-5">
                  {hourlyChart.every((h) => h.seconds === 0) ? <EmptyState /> : (
                    <ResponsiveContainer width="100%" height={210}>
                      <BarChart data={hourlyChart} barSize={18} barCategoryGap="34%" margin={{ top: 8, right: 8, left: -14, bottom: 0 }}>
                        <defs>
                          <linearGradient id="hourBar" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#5ee0a0" />
                            <stop offset="100%" stopColor="#bbf7d0" />
                          </linearGradient>
                          <linearGradient id="hourBarPeak" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor={ACCENT_DEEP} />
                            <stop offset="100%" stopColor="#74e6ae" />
                          </linearGradient>
                        </defs>
                        <CartesianGrid stroke={GRID} vertical={false} strokeDasharray="4 8" />
                        <XAxis
                          dataKey="hour"
                          interval={0}
                          tick={{ fontSize: 11, fill: AXIS }}
                          tickFormatter={(v) => (Number(v) % 3 === 0 ? String(v) : "")}
                          axisLine={false}
                          tickLine={false}
                          minTickGap={0}
                        />
                        <YAxis
                          tick={{ fontSize: 11, fill: AXIS }}
                          tickFormatter={(v) => formatHourAxisTick(Number(v))}
                          axisLine={false}
                          tickLine={false}
                          domain={[0, 3600]}
                          ticks={[0, 900, 1800, 2700, 3600]}
                          width={52}
                        />
                        <Tooltip cursor={{ fill: "rgba(16, 185, 129, 0.08)", radius: 6 }} content={<DurationTooltip />} />
                        <Bar
                          dataKey="seconds"
                          radius={[5, 5, 3, 3]}
                          animationBegin={80}
                          animationDuration={720}
                          animationEasing="ease-out"
                        >
                          {hourlyChart.map((entry) => (
                            <Cell
                              key={entry.label}
                              fill={entry.seconds === 0 ? "transparent" : entry.isPeak ? "url(#hourBarPeak)" : "url(#hourBar)"}
                            />
                          ))}
                        </Bar>
                      </BarChart>
                    </ResponsiveContainer>
                  )}
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="pb-0"><CardTitle>应用 TOP10</CardTitle></CardHeader>
                <CardContent className="p-0 pt-2">
                  {daily.top_apps.length === 0 ? <div className="px-4 pb-4"><EmptyState /></div> : (
                    daily.top_apps.map((app, i) => (
                      <div key={app.app_name} className="list-row">
                        <span className="flex min-w-0 items-center gap-3 text-[13px]">
                          <span className="w-5 text-[11px] font-bold text-muted-foreground">{String(i + 1).padStart(2, "0")}</span>
                          <AppIcon
                            name={app.app_name}
                            iconDataUrl={app.icon_data_url}
                            size="sm"
                            fallbackClassName="rounded-lg bg-gradient-to-br from-slate-400 to-slate-600"
                          />
                          <span className="truncate">{app.app_name}</span>
                        </span>
                        <span className="stat-value shrink-0 text-[13px] font-semibold text-primary">{formatDurationShort(app.seconds)}</span>
                      </div>
                    ))
                  )}
                </CardContent>
              </Card>
            </div>
          )}
        </TabsContent>

        <TabsContent value="weekly">
          {!weekly ? <EmptyState /> : (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-3">
                <MiniStat label="日均时长" value={formatDuration(weekly.average_seconds)} highlight />
                <MiniStat label="每日建议上限" value={formatLimitDuration(weekly.daily_limit_seconds)} />
              </div>
              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-0">
                  <CardTitle>7 日对比</CardTitle>
                  {weeklyPeak.seconds > 0 && (
                    <span className="rounded-md bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary">
                      峰值 {weeklyPeak.label} · {formatDurationShort(weeklyPeak.seconds)}
                    </span>
                  )}
                </CardHeader>
                <CardContent className="pt-5">
                  {weeklyChart.every((d) => d.seconds === 0) ? <EmptyState /> : (
                    <>
                      <ResponsiveContainer width="100%" height={210}>
                        <BarChart data={weeklyChart} barSize={28} barCategoryGap="30%" margin={{ top: 8, right: 8, left: -14, bottom: 0 }}>
                          <defs>
                            <linearGradient id="weekBar" x1="0" y1="0" x2="0" y2="1">
                              <stop offset="0%" stopColor="#5ee0a0" />
                              <stop offset="100%" stopColor="#bbf7d0" />
                            </linearGradient>
                            <linearGradient id="weekBarPeak" x1="0" y1="0" x2="0" y2="1">
                              <stop offset="0%" stopColor={ACCENT_DEEP} />
                              <stop offset="100%" stopColor="#74e6ae" />
                            </linearGradient>
                          </defs>
                          <CartesianGrid stroke={GRID} vertical={false} strokeDasharray="4 8" />
                          <XAxis dataKey="date" tick={{ fontSize: 11, fill: AXIS }} axisLine={false} tickLine={false} />
                          <YAxis
                            tick={{ fontSize: 11, fill: AXIS }}
                            tickFormatter={(v) => formatWeeklyAxisTick(Number(v), weeklyAxis.max)}
                            axisLine={false}
                            tickLine={false}
                            domain={[0, weeklyAxis.max]}
                            ticks={weeklyAxis.ticks}
                            width={52}
                          />
                          <Tooltip cursor={{ fill: "rgba(16, 185, 129, 0.08)", radius: 6 }} content={<DurationTooltip />} />
                          <Bar
                            dataKey="seconds"
                            radius={[5, 5, 3, 3]}
                            animationBegin={80}
                            animationDuration={720}
                            animationEasing="ease-out"
                          >
                            {weeklyChart.map((entry) => (
                              <Cell
                                key={entry.date}
                                fill={entry.seconds === 0 ? "transparent" : entry.date === weeklyPeak.date ? "url(#weekBarPeak)" : "url(#weekBar)"}
                              />
                            ))}
                          </Bar>
                        </BarChart>
                      </ResponsiveContainer>
                      {weekly.days.some((d) => d.is_over_limit) && (
                        <div className="mt-3 flex flex-wrap gap-2">
                          {weekly.days.filter((d) => d.is_over_limit).map((d) => (
                            <span key={d.date} className="rounded-md bg-amber-400/16 px-2.5 py-1 text-[11px] font-medium text-amber-700 dark:text-amber-300">
                              {d.date.slice(5)} 超出上限
                            </span>
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </CardContent>
              </Card>
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}

interface DurationTooltipProps {
  active?: boolean;
  label?: string | number;
  payload?: Array<{
    value?: number | string;
    payload?: {
      label?: string;
    };
  }>;
}

function DurationTooltip({ active, label, payload }: DurationTooltipProps) {
  if (!active || !payload?.length) return null;

  const seconds = Number(payload[0].value ?? 0);
  const displayLabel = payload[0].payload?.label ?? label;

  return (
    <div className="rounded-lg border border-border/80 bg-popover/92 px-3 py-2 text-[12px] shadow-lg shadow-emerald-950/5 backdrop-blur">
      <p className="font-semibold text-foreground">{displayLabel}</p>
      <p className="mt-1 text-muted-foreground">
        使用时长 <span className="font-semibold text-primary">{formatDuration(seconds)}</span>
      </p>
    </div>
  );
}

function formatHourAxisTick(seconds: number) {
  if (seconds <= 0) return "0m";
  return `${Math.round(seconds / 60)}m`;
}

function getWeeklyAxis(values: number[]) {
  const maxValue = Math.max(0, ...values);

  if (maxValue <= 3600) {
    return {
      max: 3600,
      ticks: [0, 900, 1800, 2700, 3600],
    };
  }

  const maxHours = Math.ceil(maxValue / 3600);
  const tickStepHours = maxHours <= 6 ? 1 : maxHours <= 12 ? 2 : 4;
  const maxTickHours = Math.ceil(maxHours / tickStepHours) * tickStepHours;
  const tickStep = tickStepHours * 3600;
  const max = maxTickHours * 3600;
  const ticks: number[] = [];

  for (let value = 0; value <= max; value += tickStep) {
    ticks.push(value);
  }

  return { max, ticks };
}

function formatWeeklyAxisTick(seconds: number, axisMax: number) {
  if (axisMax <= 3600) return formatHourAxisTick(seconds);
  if (seconds <= 0) return "0h";
  return `${Math.round(seconds / 3600)}h`;
}

function formatLimitDuration(seconds: number) {
  return formatDuration(seconds);
}

function MiniStat({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <Card className={highlight ? "ring-1 ring-primary/30" : undefined}>
      <CardContent className="p-3.5">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">{label}</p>
        <p className={`stat-value mt-1 truncate text-[15px] font-bold ${highlight ? "gradient-text" : ""}`}>{value}</p>
      </CardContent>
    </Card>
  );
}

function EmptyState() {
  return <p className="py-10 text-center text-[13px] text-muted-foreground">暂无数据</p>;
}
