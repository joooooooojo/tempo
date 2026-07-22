import { AlertTriangle } from "lucide-react";

/**
 * Placeholder host for plugin-webview apps until Rust raw Wry child views land.
 * Keeps the palette shell usable while Phase 1 UI surface is implemented.
 */
export function PluginAppHost({
  pluginId,
  entryPath,
}: {
  pluginId?: string;
  entryPath: string;
  params?: Record<string, unknown>;
}) {
  return (
    <div className="flex h-full min-h-[240px] flex-col items-center justify-center gap-3 px-6 text-center text-muted-foreground">
      <AlertTriangle className="size-8 opacity-60" aria-hidden="true" />
      <div className="space-y-1">
        <p className="text-sm font-medium text-foreground">插件视图尚未接入</p>
        <p className="text-xs leading-relaxed">
          {pluginId ? `${pluginId} · ` : ""}
          {entryPath}
        </p>
        <p className="text-xs">后续将在快捷面板内以隔离 WebView 加载此入口。</p>
      </div>
    </div>
  );
}
