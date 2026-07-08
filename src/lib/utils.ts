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
