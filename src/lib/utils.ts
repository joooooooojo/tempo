import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const isMacTarget = __TAURI_TARGET_PLATFORM__ === "macos";

export function getDurationParts(seconds: number) {
  const totalSeconds = Math.max(0, Math.floor(seconds));
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;

  if (h > 0) {
    return [
      { value: h, unit: "小时", shortUnit: "h" },
      { value: m, unit: "分钟", shortUnit: "m" },
    ];
  }

  return [
    { value: m, unit: "分钟", shortUnit: "m" },
    { value: s, unit: "秒", shortUnit: "s" },
  ];
}

export function formatDuration(seconds: number): string {
  return getDurationParts(seconds)
    .map((part) => `${part.value}${part.unit}`)
    .join("");
}

export function formatDurationShort(seconds: number): string {
  return getDurationParts(seconds)
    .map((part) => `${part.value}${part.shortUnit}`)
    .join(" ");
}

export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function formatRelativeTime(iso: string): string {
  const timestamp = new Date(iso).getTime();
  if (Number.isNaN(timestamp)) return iso;
  const diff = Date.now() - timestamp;
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < minute) return "刚刚";
  if (diff < hour) return `${Math.floor(diff / minute)} 分钟前`;
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`;
  if (diff < 7 * day) return `${Math.floor(diff / day)} 天前`;
  return new Date(iso).toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

export function appBadgeLabel(name?: string | null): string {
  if (!name) return "—";
  const trimmed = name.trim();
  if (!trimmed) return "—";
  return trimmed.slice(0, 2).toUpperCase();
}

export function previewLines(text: string, maxLines = 4): string {
  return text.split(/\r?\n/).slice(0, maxLines).join("\n");
}
