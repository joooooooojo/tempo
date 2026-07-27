import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle, LoaderCircle } from "lucide-react";
import { PluginAppHost } from "@/apps/PluginAppHost";
import { useAuxiliaryWindowShell } from "@/hooks/useAuxiliaryWindow";
import { api } from "@/lib/api";
import type { PluginWindowContext } from "@/types";

function normalizeParams(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

export function PluginWindowPage() {
  useAuxiliaryWindowShell("plugin-window");
  const [context, setContext] = useState<PluginWindowContext | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    api
      .pluginWindowContext()
      .then((next) => {
        if (!disposed) setContext(next);
      })
      .catch((reason) => {
        if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      disposed = true;
    };
  }, []);

  const closeWindow = useCallback(() => {
    void getCurrentWindow().close();
  }, []);

  if (error) {
    return (
      <main className="flex h-screen w-screen flex-col items-center justify-center gap-3 bg-background px-6 text-center text-muted-foreground">
        <AlertTriangle className="size-8 opacity-60" aria-hidden="true" />
        <p className="text-sm font-medium text-foreground">插件窗口加载失败</p>
        <p className="max-w-lg text-xs leading-relaxed">{error}</p>
      </main>
    );
  }

  if (!context) {
    return (
      <main className="flex h-screen w-screen items-center justify-center bg-background text-muted-foreground">
        <LoaderCircle className="size-6 animate-spin" aria-hidden="true" />
      </main>
    );
  }

  return (
    <main className="h-screen w-screen overflow-hidden bg-background text-foreground">
      <PluginAppHost
        pluginId={context.pluginId}
        appId={context.appId}
        params={normalizeParams(context.params)}
        onBack={closeWindow}
      />
    </main>
  );
}
