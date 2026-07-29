import { Badge } from "@/components/ui/badge";
import type { PluginDevConnectionStatus, PluginDevPreferences } from "@/types";

export type WorkspaceTab = "manifest" | "connection" | "test";
export type ManifestMode = "visual" | "json";

export const KIND_ITEMS = [
  { value: "ui", label: "UI" },
  { value: "headless", label: "Headless" },
  { value: "hybrid", label: "Hybrid" },
] as const;
export const UI_SOURCE_ITEMS = [
  { value: "url", label: "服务 URL" },
  { value: "static", label: "静态目录" },
] as const;
export const WINDOW_MODE_ITEMS = [
  { value: "normal", label: "主面板" },
  { value: "standalone", label: "独立窗口" },
] as const;
export const TARGET_ITEMS = [
  { value: "app", label: "打开 App" },
  { value: "command", label: "调用 Command" },
] as const;
export const SETTING_TYPE_ITEMS = [
  { value: "switch", label: "开关" },
  { value: "input", label: "输入框" },
  { value: "select", label: "单选" },
  { value: "multiselect", label: "多选" },
] as const;

const PROJECT_MARK_COLORS = [
  "#2563eb",
  "#0f766e",
  "#7c3aed",
  "#be123c",
  "#0369a1",
  "#4338ca",
  "#a21caf",
  "#047857",
] as const;

export function projectFolderName(rootPath: string): string {
  const normalized = rootPath.replace(/[\\/]+$/, "");
  const segments = normalized.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? normalized;
}

export function projectMonogram(rootPath: string): string {
  const folderName = projectFolderName(rootPath);
  const parts = folderName.split("-").filter(Boolean);
  const first = Array.from(parts[0] ?? folderName)[0] ?? "?";
  const lastPart = parts[parts.length - 1] ?? folderName;
  const lastCharacters = Array.from(lastPart);
  const last =
    parts.length > 1
      ? (lastCharacters[0] ?? first)
      : (lastCharacters[lastCharacters.length - 1] ?? first);
  return `${first}${last}`.toLocaleLowerCase();
}

export function projectMarkColor(rootPath: string): string {
  let hash = 0;
  for (const character of rootPath.toLocaleLowerCase()) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }
  return PROJECT_MARK_COLORS[hash % PROJECT_MARK_COLORS.length];
}

export function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function connectionBadge(status: PluginDevConnectionStatus) {
  if (!status.connected) return <Badge variant="outline">未连接</Badge>;
  if (status.state === "failed")
    return <Badge variant="destructive">连接失败</Badge>;
  if (status.state === "partial")
    return <Badge variant="secondary">部分连接</Badge>;
  return <Badge>已连接</Badge>;
}

export function projectKindLabel(kind?: string | null) {
  if (kind === "headless") return "Headless";
  if (kind === "hybrid") return "Hybrid";
  return "UI";
}

export function normalizePreferences(
  value: PluginDevPreferences,
): PluginDevPreferences {
  return {
    uiSourceKind: value.uiSourceKind ?? "url",
    uiServiceUrl: value.uiServiceUrl ?? "http://127.0.0.1:5173/",
    uiStaticRoot: value.uiStaticRoot ?? "",
    runtimeDevEntry: value.runtimeDevEntry ?? "",
    autoReconnectRuntime: value.autoReconnectRuntime ?? true,
    receiveRealHooks: value.receiveRealHooks ?? false,
    useProductionData: value.useProductionData ?? false,
  };
}
