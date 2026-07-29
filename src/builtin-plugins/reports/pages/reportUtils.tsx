import { useEffect, useState } from "react";
import { addDays, format, isToday, isYesterday, parseISO } from "date-fns";
import { zhCN } from "date-fns/locale";

export function formatHourAxisTick(seconds: number) {
  if (seconds <= 0) return "0m";
  return `${Math.round(seconds / 60)}m`;
}

export function getWeeklyAxis(values: number[]) {
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

export function formatWeeklyAxisTick(seconds: number, axisMax: number) {
  if (axisMax <= 3600) return formatHourAxisTick(seconds);
  if (seconds <= 0) return "0h";
  return `${Math.round(seconds / 3600)}h`;
}

export function getTodayKey() {
  return format(new Date(), "yyyy-MM-dd");
}

export function shiftDate(date: string, amount: number) {
  return format(addDays(parseISO(date), amount), "yyyy-MM-dd");
}

export function formatReportDate(date: string) {
  const parsedDate = parseISO(date);
  const monthAndDay = format(parsedDate, "M月d日", { locale: zhCN });

  if (isToday(parsedDate)) return `今天 · ${monthAndDay}`;
  if (isYesterday(parsedDate)) return `昨天 · ${monthAndDay}`;
  if (parsedDate.getFullYear() !== new Date().getFullYear()) {
    return format(parsedDate, "yyyy/MM/dd");
  }

  return `${monthAndDay} · ${format(parsedDate, "EEE", { locale: zhCN })}`;
}

export function formatWeekRange(endDate: string) {
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

export function usePrefersReducedMotion() {
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

export function EmptyState() {
  return <p className="py-10 text-center text-[13px] text-muted-foreground">暂无数据</p>;
}
