import { useEffect, useState, type ReactNode } from "react";
import { CircleHelp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  DEFAULT_SHORTCUTS,
  formatShortcutLabel,
  shortcutFromKeyboardEvent,
} from "@/lib/shortcut";
import type { Settings } from "@/types";

export const CLIPBOARD_RETENTION_OPTIONS = [
  { value: "days", label: "天" },
  { value: "weeks", label: "周" },
  { value: "months", label: "个月" },
  { value: "years", label: "年" },
  { value: "permanent", label: "永久" },
] as const satisfies ReadonlyArray<{
  value: Settings["clipboard_history_retention"];
  label: string;
}>;

export const THEME_OPTIONS: Array<{ value: Settings["theme"]; label: string }> = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

export const SHORTCUT_SETTING_KEYS = [
  "shortcut_main_panel",
  "shortcut_clipboard_picker",
  "shortcut_snippet_picker",
] as const;

export type ShortcutSettingKey = (typeof SHORTCUT_SETTING_KEYS)[number];

export type SettingsSectionId = "general" | "plugins" | "storage";

export const SETTINGS_SECTIONS: Array<{
  id: SettingsSectionId;
  label: string;
}> = [
  { id: "general", label: "通用设置" },
  { id: "plugins", label: "插件管理" },
  { id: "storage", label: "存储管理" },
];

export type SettingsUpdater = (patch: Partial<Settings>) => Promise<void>;

export function normalizeShortcutForComparison(shortcut: string) {
  return shortcut.trim().toLowerCase().replace(/^ctrl\+/, "control+");
}

export function clipboardRetentionIndex(value: Settings["clipboard_history_retention"]) {
  const index = CLIPBOARD_RETENTION_OPTIONS.findIndex((option) => option.value === value);
  return index >= 0 ? index : 0;
}

export function clipboardRetentionValue(index: number): Settings["clipboard_history_retention"] {
  return CLIPBOARD_RETENTION_OPTIONS[index]?.value ?? "days";
}

export function clipboardRetentionLabel(value: Settings["clipboard_history_retention"]) {
  switch (value) {
    case "days":
      return "保留最近 1 天内的历史";
    case "weeks":
      return "保留最近 1 周内的历史";
    case "months":
      return "保留最近 1 个月内的历史";
    case "years":
      return "保留最近 1 年内的历史";
    case "permanent":
      return "永久保留历史记录";
    default:
      return "保留最近 1 天内的历史";
  }
}

export function parseSettingsSectionId(value: string | null | undefined): SettingsSectionId | null {
  if (value === "general" || value === "plugins" || value === "storage") {
    return value;
  }
  return null;
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h2 className="settings-section__title">{title}</h2>
      {children}
    </section>
  );
}

export function Row({
  label,
  desc,
  labelExtra,
  children,
}: {
  label: string;
  desc?: string;
  labelExtra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="list-row">
      <div className="min-w-0">
        <p className="inline-flex items-center gap-1.5 text-[14px] font-medium">
          {label}
          {labelExtra}
        </p>
        {desc ? <p className="text-[11px] text-muted-foreground">{desc}</p> : null}
      </div>
      {children}
    </div>
  );
}

export function ShortcutRow({
  label,
  desc,
  value,
  onChange,
}: {
  label: string;
  desc?: string;
  value: string;
  onChange: (value: string) => Promise<void>;
}) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecording(false);
        return;
      }

      const next = shortcutFromKeyboardEvent(event);
      if (!next) return;

      setRecording(false);
      void onChange(next).catch(() => undefined);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [recording, onChange]);

  return (
    <div className="list-row">
      <div className="min-w-0">
        <p className="text-[14px] font-medium">{label}</p>
        {desc ? <p className="text-[11px] text-muted-foreground">{desc}</p> : null}
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant={recording ? "default" : "outline"}
          size="sm"
          className="min-w-28 font-mono text-[12px]"
          onClick={() => setRecording((prev) => !prev)}
        >
          {recording ? "按下快捷键" : value ? formatShortcutLabel(value) : "未设置"}
        </Button>
      </div>
    </div>
  );
}

export function PasteModeOption({
  selected,
  title,
  description,
  onSelect,
}: {
  selected: boolean;
  title: string;
  description: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full items-start gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors ${
        selected
          ? "border-primary/40 bg-primary/5"
          : "border-border/60 bg-transparent hover:bg-foreground/[0.03]"
      }`}
    >
      <span
        className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${
          selected ? "border-primary" : "border-muted-foreground/40"
        }`}
      >
        {selected ? <span className="h-2 w-2 rounded-full bg-primary" /> : null}
      </span>
      <span className="min-w-0">
        <span className="block text-[13px] font-medium">{title}</span>
        <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground">
          {description}
        </span>
      </span>
    </button>
  );
}

export function McpCapabilitiesHint() {
  return (
    <Popover>
      <PopoverTrigger asChild openOnHover delay={200} closeDelay={100}>
        <button
          type="button"
          className="inline-flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          aria-label="查看 MCP 功能"
        >
          <CircleHelp className="size-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" side="bottom" initialFocus={false} className="w-72 p-3">
        <p className="mb-2 text-[13px] font-medium">MCP 可提供的能力</p>
        <ul className="space-y-1.5 text-[12px] leading-relaxed text-muted-foreground">
          <li>待办：创建、查询、更新、完成、置顶、删除、子任务与备注</li>
          <li>快捷短语：查询、创建、更新、删除、分组、复制到剪贴板</li>
          <li>剪贴板：搜索历史记录</li>
          <li>报告：读取今日屏幕使用报告</li>
          <li>插件：启用插件后，其声明的工具也会自动暴露给 MCP</li>
        </ul>
      </PopoverContent>
    </Popover>
  );
}

export { DEFAULT_SHORTCUTS };
