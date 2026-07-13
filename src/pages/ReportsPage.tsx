import { useEffect, useState } from "react";
import {
  BarChart, Bar, Cell, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AppIcon } from "@/components/AppIcon";
import { TrackingStatus } from "@/components/TrackingStatus";
import { api } from "@/lib/api";
import { formatDuration, formatDurationShort } from "@/lib/utils";
import type { DailyReport, WeeklyReport } from "@/types";

const ACCENT_DEEP = "#10b981";
const AXIS = "#6b7f78";
const GRID = "rgba(42, 84, 70, 0.1)";
const HOUR_AXIS_TICKS = [0, 900, 1800, 2700, 3600];

export function ReportsPage() {
  const [activeTab, setActiveTab] = useState<"daily" | "weekly">("daily");
  const [daily, setDaily] = useState<DailyReport | null>(null);
  const [weekly, setWeekly] = useState<WeeklyReport | null>(null);
  const [iconsReady, setIconsReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      api.getDailyReport()
        .then((report) => {
          if (!cancelled) setDaily(report);
        })
        .catch(console.error);
    };

    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (activeTab !== "weekly") return;

    let cancelled = false;
    const refresh = () => {
      api.getWeeklyReport()
        .then((report) => {
          if (!cancelled) setWeekly(report);
        })
        .catch(console.error);
    };

    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [activeTab]);

  const reportIconKey = activeTab === "daily"
    ? daily && `daily:${daily.date}:${daily.top_apps.length}`
    : weekly && `weekly:${weekly.days[0]?.date ?? ""}:${weekly.days[weekly.days.length - 1]?.date ?? ""}:${weekly.top_apps.length}`;

  useEffect(() => {
    if (!reportIconKey) {
      setIconsReady(false);
      return;
    }

    setIconsReady(false);
    const timer = window.setTimeout(() => setIconsReady(true), 180);
    return () => window.clearTimeout(timer);
  }, [reportIconKey]);

  const appIconUrl = (url?: string | null) => (iconsReady ? url : null);

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
      <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as "daily" | "weekly")}>
        <div className="mb-4 flex items-center justify-between gap-4">
          <TabsList className="w-[240px]">
            <TabsTrigger value="daily" className="min-w-0 flex-1">日报</TabsTrigger>
            <TabsTrigger value="weekly" className="min-w-0 flex-1">周报</TabsTrigger>
          </TabsList>
          <TrackingStatus className="shrink-0" />
        </div>

        <TabsContent value="daily">
          {!daily ? <EmptyState /> : (
            <div className="space-y-4">
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
                    <div className="usage-chart h-[210px]">
                      <ResponsiveContainer width="100%" height="100%">
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
                          <CartesianGrid
                            stroke={GRID}
                            vertical={false}
                            strokeDasharray="4 8"
                            syncWithTicks
                          />
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
                            ticks={HOUR_AXIS_TICKS}
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
                    </div>
                  )}
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="pb-0"><CardTitle>应用排名</CardTitle></CardHeader>
                <CardContent className="p-0 pt-2">
                  {daily.top_apps.length === 0 ? <div className="px-4 pb-4"><EmptyState /></div> : (
                    daily.top_apps.map((app, i) => (
                      <div key={app.app_name} className="list-row">
                        <span className="flex min-w-0 items-center gap-3 text-[13px]">
                          <span className="w-5 text-[11px] font-bold text-muted-foreground">{String(i + 1).padStart(2, "0")}</span>
                          <AppIcon
                            name={app.app_name}
                            iconDataUrl={appIconUrl(app.icon_data_url)}
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
                      <div className="usage-chart h-[210px]">
                        <ResponsiveContainer width="100%" height="100%">
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
                            <CartesianGrid
                              stroke={GRID}
                              vertical={false}
                              strokeDasharray="4 8"
                            />
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
                      </div>
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

              <Card>
                <CardHeader className="pb-0"><CardTitle>应用排名</CardTitle></CardHeader>
                <CardContent className="p-0 pt-2">
                  {weekly.top_apps.length === 0 ? <div className="px-4 pb-4"><EmptyState /></div> : (
                    weekly.top_apps.map((app, i) => (
                      <div key={app.app_name} className="list-row">
                        <span className="flex min-w-0 items-center gap-3 text-[13px]">
                          <span className="w-5 text-[11px] font-bold text-muted-foreground">{String(i + 1).padStart(2, "0")}</span>
                          <AppIcon
                            name={app.app_name}
                            iconDataUrl={appIconUrl(app.icon_data_url)}
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

function EmptyState() {
  return <p className="py-10 text-center text-[13px] text-muted-foreground">暂无数据</p>;
}
