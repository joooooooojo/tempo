import {
  startTransition,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  BarChart, Bar, Cell, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TrackingStatus } from "@/builtin-plugins/reports/components/TrackingStatus";
import { AppRankingCard, DurationTooltip } from "@/builtin-plugins/reports/pages/AppRankingCard";
import { ReportPeriodNavigation } from "@/builtin-plugins/reports/pages/ReportPeriodNavigation";
import {
  EmptyState,
  formatHourAxisTick,
  formatReportDate,
  formatWeekRange,
  formatWeeklyAxisTick,
  getTodayKey,
  getWeeklyAxis,
  shiftDate,
  usePrefersReducedMotion,
} from "@/builtin-plugins/reports/pages/reportUtils";
import { api } from "@/lib/api";
import { formatDurationShort } from "@/lib/utils";
import type { DailyReport, WeeklyReport } from "@/types";

const ACCENT_DEEP = "#10b981";
const AXIS = "#6b7f78";
const GRID = "rgba(42, 84, 70, 0.1)";
const HOUR_AXIS_TICKS = [0, 900, 1800, 2700, 3600];

export function ReportsPage() {
  const prefersReducedMotion = usePrefersReducedMotion();
  const [activeTab, setActiveTab] = useState<"daily" | "weekly">("daily");
  const [selectedDate, setSelectedDate] = useState(getTodayKey);
  const [selectedWeekEndDate, setSelectedWeekEndDate] = useState(getTodayKey);
  const [daily, setDaily] = useState<DailyReport | null>(null);
  const [weekly, setWeekly] = useState<WeeklyReport | null>(null);

  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      api.getDailyReport(selectedDate)
        .then((report) => {
          if (!cancelled) startTransition(() => setDaily(report));
        })
        .catch(console.error);
    };

    refresh();
    const timer = selectedDate === getTodayKey()
      ? window.setInterval(refresh, 60_000)
      : null;
    return () => {
      cancelled = true;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [selectedDate]);

  useEffect(() => {
    if (activeTab !== "weekly") return;

    let cancelled = false;
    const refresh = () => {
      api.getWeeklyReport(selectedWeekEndDate)
        .then((report) => {
          if (!cancelled) startTransition(() => setWeekly(report));
        })
        .catch(console.error);
    };

    refresh();
    const timer = selectedWeekEndDate === getTodayKey()
      ? window.setInterval(refresh, 60_000)
      : null;
    return () => {
      cancelled = true;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [activeTab, selectedWeekEndDate]);

  const todayKey = getTodayKey();
  const isViewingToday = selectedDate >= todayKey;
  const isViewingCurrentWeek = selectedWeekEndDate >= todayKey;

  const hourlyChart = useMemo(() => daily?.hourly.map((h) => ({
    hour: h.hour,
    label: `${String(h.hour).padStart(2, "0")}:00`,
    seconds: h.seconds,
    isPeak: h.seconds > 0 && h.hour === daily.peak_hour,
  })) ?? [], [daily]);
  const weeklyChart = useMemo(() => weekly?.days.map((d, slot) => ({
    slot,
    date: d.date.slice(5),
    label: d.date.slice(5),
    seconds: d.seconds,
    isOverLimit: d.is_over_limit,
  })) ?? [], [weekly]);
  const weeklyPeak = useMemo(() => weeklyChart.reduce(
    (peak, day) => (day.seconds > peak.seconds ? day : peak),
    { date: "", label: "", seconds: 0, isOverLimit: false }
  ), [weeklyChart]);
  const weeklyAxis = useMemo(
    () => getWeeklyAxis(weeklyChart.map((day) => day.seconds)),
    [weeklyChart]
  );

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
                <CardHeader className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 pb-0 max-[640px]:grid-cols-[auto_1fr]">
                  <CardTitle className="justify-self-start">每小时趋势</CardTitle>
                  <ReportPeriodNavigation
                    label={formatReportDate(selectedDate)}
                    dateTime={selectedDate}
                    groupLabel="日报日期切换"
                    previousLabel="前一天"
                    nextLabel="后一天"
                    nextDisabledLabel="已是今天"
                    nextDisabled={isViewingToday}
                    onPrevious={() => setSelectedDate((date) => shiftDate(date, -1))}
                    onNext={() => setSelectedDate((date) => shiftDate(date, 1))}
                  />
                  {daily.peak_seconds > 0 && (
                    <span className="justify-self-end rounded-md bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary max-[640px]:col-start-2 max-[640px]:row-start-1">
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
                            isAnimationActive={!prefersReducedMotion}
                            animationBegin={0}
                            animationDuration={420}
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

              <AppRankingCard apps={daily.top_apps} periodKey={daily.date} />
            </div>
          )}
        </TabsContent>

        <TabsContent value="weekly">
          {!weekly ? <EmptyState /> : (
            <div className="space-y-4">
              <Card>
                <CardHeader className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 pb-0 max-[640px]:grid-cols-[auto_1fr]">
                  <CardTitle className="justify-self-start">7 日对比</CardTitle>
                  <ReportPeriodNavigation
                    label={formatWeekRange(selectedWeekEndDate)}
                    dateTime={selectedWeekEndDate}
                    groupLabel="周报日期切换"
                    previousLabel="前 7 天"
                    nextLabel="后 7 天"
                    nextDisabledLabel="已是当前周期"
                    nextDisabled={isViewingCurrentWeek}
                    onPrevious={() => setSelectedWeekEndDate((date) => shiftDate(date, -7))}
                    onNext={() => setSelectedWeekEndDate((date) => shiftDate(date, 7))}
                  />
                  {weeklyPeak.seconds > 0 && (
                    <span className="justify-self-end rounded-md bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary max-[640px]:col-start-2 max-[640px]:row-start-1">
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
                              isAnimationActive={!prefersReducedMotion}
                              animationBegin={0}
                              animationDuration={420}
                              animationEasing="ease-out"
                            >
                              {weeklyChart.map((entry) => (
                                <Cell
                                  key={entry.slot}
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

              <AppRankingCard
                apps={weekly.top_apps}
                periodKey={`${weekly.days[0]?.date ?? ""}:${weekly.days[weekly.days.length - 1]?.date ?? ""}`}
              />
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
