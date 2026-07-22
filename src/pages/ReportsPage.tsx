import {
  memo,
  startTransition,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { addDays, format, isToday, isYesterday, parseISO } from "date-fns";
import { zhCN } from "date-fns/locale";
import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  BarChart, Bar, Cell, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AppIcon } from "@/components/AppIcon";
import { TrackingStatus } from "@/components/TrackingStatus";
import { api } from "@/lib/api";
import { formatDuration, formatDurationShort } from "@/lib/utils";
import type { AppUsage, DailyReport, WeeklyReport } from "@/types";

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

interface ReportPeriodNavigationProps {
  label: string;
  dateTime: string;
  groupLabel: string;
  previousLabel: string;
  nextLabel: string;
  nextDisabledLabel: string;
  nextDisabled: boolean;
  onPrevious: () => void;
  onNext: () => void;
}

function ReportPeriodNavigation({
  label,
  dateTime,
  groupLabel,
  previousLabel,
  nextLabel,
  nextDisabledLabel,
  nextDisabled,
  onPrevious,
  onNext,
}: ReportPeriodNavigationProps) {
  return (
    <div
      className="flex items-center gap-1 justify-self-center rounded-lg border border-border/70 bg-muted/45 p-0.5 max-[640px]:col-span-2 max-[640px]:col-start-1 max-[640px]:row-start-2"
      role="group"
      aria-label={groupLabel}
    >
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={`查看${previousLabel}`}
        title={previousLabel}
        onClick={onPrevious}
      >
        <ChevronLeft />
      </Button>
      <time
        dateTime={dateTime}
        className="w-[132px] text-center text-[12px] font-semibold tabular-nums text-foreground"
        aria-live="polite"
      >
        {label}
      </time>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={`查看${nextLabel}`}
        title={nextDisabled ? nextDisabledLabel : nextLabel}
        disabled={nextDisabled}
        onClick={onNext}
      >
        <ChevronRight />
      </Button>
    </div>
  );
}

interface AppRankingCardProps {
  apps: AppUsage[];
  periodKey: string;
}

const AppRankingCard = memo(function AppRankingCard({
  apps,
  periodKey,
}: AppRankingCardProps) {
  const contentRef = useRef<HTMLDivElement>(null);
  const previousPositionsRef = useRef<Map<string, DOMRect>>(new Map());
  const previousPeriodRef = useRef<string | null>(null);

  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content) return;

    const rows = Array.from(content.querySelectorAll<HTMLElement>("[data-app-key]"));
    const nextPositions = new Map(
      rows.map((row) => [row.dataset.appKey ?? "", row.getBoundingClientRect()])
    );
    const shouldAnimate = previousPeriodRef.current !== null
      && previousPeriodRef.current !== periodKey
      && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const animations: Animation[] = [];

    if (shouldAnimate) {
      rows.forEach((row, index) => {
        const appKey = row.dataset.appKey ?? "";
        const previousRect = previousPositionsRef.current.get(appKey);
        const nextRect = nextPositions.get(appKey);
        const delay = Math.min(index * 22, 110);

        if (previousRect && nextRect) {
          const deltaX = previousRect.left - nextRect.left;
          const deltaY = previousRect.top - nextRect.top;

          if (Math.abs(deltaX) > 0.5 || Math.abs(deltaY) > 0.5) {
            animations.push(row.animate(
              [
                { transform: `translate(${deltaX}px, ${deltaY}px)` },
                { transform: "translate(0, 0)" },
              ],
              {
                duration: 420,
                delay,
                easing: "cubic-bezier(0.22, 1, 0.36, 1)",
              }
            ));
          }
          return;
        }

        animations.push(row.animate(
          [
            { opacity: 0, transform: "translateY(8px) scale(0.985)" },
            { opacity: 1, transform: "translateY(0) scale(1)" },
          ],
          {
            duration: 300,
            delay,
            easing: "cubic-bezier(0.22, 1, 0.36, 1)",
          }
        ));
      });
    }

    previousPositionsRef.current = nextPositions;
    previousPeriodRef.current = periodKey;

    return () => animations.forEach((animation) => animation.cancel());
  }, [apps, periodKey]);

  return (
    <Card>
      <CardHeader className="pb-0"><CardTitle>应用排名</CardTitle></CardHeader>
      <CardContent ref={contentRef} className="p-0 pt-2">
        {apps.length === 0 ? <div className="px-4 pb-4"><EmptyState /></div> : (
          apps.map((app, index) => (
            <div key={app.app_name} data-app-key={app.app_name} className="list-row">
              <span className="flex min-w-0 items-center gap-3 text-[13px]">
                <span className="w-5 text-[11px] font-bold text-muted-foreground">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <AppIcon
                  name={app.app_name}
                  iconDataUrl={app.icon_data_url}
                  size="sm"
                  fallbackClassName="bg-gradient-to-br from-slate-400 to-slate-600 text-white"
                />
                <span className="truncate">{app.app_name}</span>
              </span>
              <span className="stat-value shrink-0 text-[13px] font-semibold text-primary">
                {formatDurationShort(app.seconds)}
              </span>
            </div>
          ))
        )}
      </CardContent>
    </Card>
  );
});

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

function getTodayKey() {
  return format(new Date(), "yyyy-MM-dd");
}

function shiftDate(date: string, amount: number) {
  return format(addDays(parseISO(date), amount), "yyyy-MM-dd");
}

function formatReportDate(date: string) {
  const parsedDate = parseISO(date);
  const monthAndDay = format(parsedDate, "M月d日", { locale: zhCN });

  if (isToday(parsedDate)) return `今天 · ${monthAndDay}`;
  if (isYesterday(parsedDate)) return `昨天 · ${monthAndDay}`;
  if (parsedDate.getFullYear() !== new Date().getFullYear()) {
    return format(parsedDate, "yyyy/MM/dd");
  }

  return `${monthAndDay} · ${format(parsedDate, "EEE", { locale: zhCN })}`;
}

function formatWeekRange(endDate: string) {
  const end = parseISO(endDate);
  const start = addDays(end, -6);

  if (start.getFullYear() !== end.getFullYear()) {
    return `${format(start, "yyyy/M/d")}–${format(end, "yyyy/M/d")}`;
  }
  if (end.getFullYear() !== new Date().getFullYear()) {
    return `${format(start, "yyyy/M/d")}–${format(end, "M/d")}`;
  }
  if (start.getMonth() === end.getMonth()) {
    return `${format(start, "M月d日")}–${format(end, "d日")}`;
  }

  return `${format(start, "M月d日")}–${format(end, "M月d日")}`;
}

function usePrefersReducedMotion() {
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(() =>
    typeof window !== "undefined"
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handleChange = () => setPrefersReducedMotion(mediaQuery.matches);

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  return prefersReducedMotion;
}

function EmptyState() {
  return <p className="py-10 text-center text-[13px] text-muted-foreground">暂无数据</p>;
}
