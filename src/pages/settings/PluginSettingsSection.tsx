import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import type {
  InstalledPlugin,
  PluginMcpToolInfo,
  PluginRuntimeStatus,
  RuntimeInstallProgress,
} from "@/types";

const NODE_RUNTIME_TRUST_TEXT =
  "启用此插件将允许其在本机执行代码，权限与 Tempo 相近（可读写文件、访问网络、发起进程等），请仅安装信任的来源。确定信任并继续？";
const UI_ONLY_TRUST_TEXT =
  "将在隔离视图中运行网页代码，并可调用受限的 Tempo 接口（面板控制、主题、私有存储等），不具备完整系统权限。确定信任并继续？";

const RUNTIME_PROGRESS_EVENT = "plugin-runtime-install-progress";

export function PluginSettingsSection() {
  const [runtime, setRuntime] = useState<PluginRuntimeStatus | null>(null);
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [busy, setBusy] = useState(false);
  const [mcpTools, setMcpTools] = useState<Record<string, PluginMcpToolInfo[]>>({});
  const [progress, setProgress] = useState<RuntimeInstallProgress | null>(null);
  const toastDoneRef = useRef(false);

  const refresh = useCallback(async () => {
    const [nextRuntime, nextPlugins] = await Promise.all([
      api.getPluginRuntimeStatus(),
      api.listPlugins(),
    ]);
    setRuntime(nextRuntime);
    setPlugins(nextPlugins);
    if (nextRuntime.progress) {
      setProgress(nextRuntime.progress);
    } else if (!nextRuntime.installing) {
      setProgress(null);
    }
    return nextRuntime;
  }, []);

  useEffect(() => {
    refresh().catch((error) => {
      console.error(error);
    });
  }, [refresh]);

  // Keep showing live progress even after closing/reopening the palette mid-install.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listen<RuntimeInstallProgress>(RUNTIME_PROGRESS_EVENT, (event) => {
      if (cancelled) return;
      const next = event.payload;
      setProgress(next);
      setRuntime((prev) =>
        prev
          ? {
              ...prev,
              installing: next.phase !== "failed" && next.phase !== "done",
              message: next.message,
              progress: next,
            }
          : prev
      );
      if (next.phase === "done") {
        if (!toastDoneRef.current) {
          toastDoneRef.current = true;
          toast.success("插件运行时已安装");
        }
        void refresh();
      } else if (next.phase === "failed") {
        toast.error(next.message || "插件运行时安装失败");
        void refresh();
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refresh]);

  // Poll while installing so remounted UI converges even if an event was missed.
  useEffect(() => {
    if (!runtime?.installing) return;
    toastDoneRef.current = false;
    const timer = window.setInterval(() => {
      void refresh().then((next) => {
        if (next.installed && !toastDoneRef.current) {
          toastDoneRef.current = true;
          toast.success("插件运行时已安装");
        }
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [runtime?.installing, refresh]);

  useEffect(() => {
    const toFetch = plugins.filter((plugin) => plugin.mcpToolCount > 0 && !(plugin.id in mcpTools));
    if (toFetch.length === 0) return;
    void Promise.all(
      toFetch.map(async (plugin) => {
        try {
          const tools = await api.listPluginMcpTools(plugin.id);
          setMcpTools((prev) => ({ ...prev, [plugin.id]: tools }));
        } catch (error) {
          console.error(error);
        }
      })
    );
  }, [plugins, mcpTools]);

  const installRuntime = async () => {
    setBusy(true);
    toastDoneRef.current = false;
    try {
      const next = await api.installPluginRuntime();
      setRuntime(next);
      if (next.progress) setProgress(next.progress);
      if (next.installed) {
        toast.success("插件运行时已安装");
      } else if (!next.installing && next.progress?.phase === "failed") {
        toast.error(next.progress.message || next.message);
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const uninstallRuntime = async () => {
    if (!confirm("卸载后，含 main 的第三方插件将无法激活。确定继续？")) return;
    setBusy(true);
    try {
      const next = await api.uninstallPluginRuntime();
      setRuntime(next);
      setProgress(null);
      toast.success("已卸载插件运行时");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const importPluginFrom = async (mode: "directory" | "zip") => {
    try {
      const selected =
        mode === "directory"
          ? await openNativeFileDialog({
              directory: true,
              multiple: false,
              title: "选择插件目录（含 manifest.json）",
            })
          : await openNativeFileDialog({
              directory: false,
              multiple: false,
              title: "选择插件 .zip 包",
              filters: [{ name: "Plugin package", extensions: ["zip"] }],
            });
      if (!selected || Array.isArray(selected)) return;
      setBusy(true);
      try {
        const installed = await api.importLocalPlugin(selected);
        toast.success(`已导入 ${installed.pluginId}@${installed.version}（尚未信任）`);
        await refresh();
      } finally {
        setBusy(false);
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      setBusy(false);
    }
  };

  const openDataDir = async (pluginId: string) => {
    try {
      await api.pluginOpenDataDir(pluginId);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const trustPlugin = async (plugin: InstalledPlugin) => {
    const confirmText = plugin.requiresNodeRuntime ? NODE_RUNTIME_TRUST_TEXT : UI_ONLY_TRUST_TEXT;
    if (!confirm(confirmText)) return;
    setBusy(true);
    try {
      await api.trustPlugin(plugin.id, plugin.currentVersion, true);
      await refresh();
      toast.success("已信任该插件包");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const setMcpExposed = async (pluginId: string, exposed: boolean) => {
    setBusy(true);
    try {
      await api.setPluginMcpExposed(pluginId, exposed);
      await refresh();
      toast.success(exposed ? "已向 MCP 暴露该插件工具" : "已停止向 MCP 暴露该插件工具");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const uninstallPlugin = async (pluginId: string) => {
    // Use !! so a Promise/object can never leak into the IPC payload (Tauri expects bool).
    const deleteData = !!window.confirm(
      "同时删除该插件的私有数据（存储 / 会话）？点击“取消”仅卸载安装包。"
    );
    setBusy(true);
    try {
      await api.pluginUninstall(pluginId, deleteData);
      toast.success("已卸载插件");
      await refresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <Card>
        <CardContent className="space-y-3 p-4">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium">插件运行时（Node）</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                {progress?.message ?? runtime?.message ?? "正在检测…"}
              </p>
              {runtime?.installed ? (
                <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                  v{runtime.version} · {runtime.nodePath}
                </p>
              ) : (
                <p className="mt-1 text-[11px] text-muted-foreground">
                  与系统 Node 无关；仅在使用含 main 的第三方插件时需要安装。关闭面板不会中断下载。
                </p>
              )}
              {runtime?.installing || (progress && progress.phase !== "failed" && progress.phase !== "done") ? (
                <div className="mt-3 space-y-1.5">
                  <Progress value={progress?.percent ?? null} className="w-full">
                    <span className="sr-only">安装进度</span>
                  </Progress>
                  <p className="text-[11px] text-muted-foreground">
                    {typeof progress?.percent === "number"
                      ? `${progress.percent}%`
                      : progress?.phase === "extracting"
                        ? "解压中…"
                        : progress?.phase === "verifying"
                          ? "校验中…"
                          : "下载中…"}
                  </p>
                </div>
              ) : null}
              {progress?.phase === "failed" ? (
                <p className="mt-2 rounded-md bg-destructive/10 px-2 py-1 text-[11px] text-destructive">
                  {progress.message}
                </p>
              ) : null}
            </div>
            <div className="flex shrink-0 gap-2">
              {runtime?.installed ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy || Boolean(runtime.installing)}
                  onClick={() => void uninstallRuntime()}
                >
                  卸载
                </Button>
              ) : (
                <Button
                  size="sm"
                  disabled={busy || Boolean(runtime?.installing)}
                  onClick={() => void installRuntime()}
                >
                  {runtime?.installing ? "安装中…" : "安装"}
                </Button>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-3 p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-[14px] font-medium">已安装插件</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                有 UI 需根级 index.html；无 UI 需根级 main.js / main.mjs。导入后需先信任，再启用。
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void importPluginFrom("directory")}
              >
                导入目录
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void importPluginFrom("zip")}
              >
                导入 .zip
              </Button>
            </div>
          </div>

          {plugins.length === 0 ? (
            <p className="text-[12px] text-muted-foreground">暂无插件</p>
          ) : (
            <div className="space-y-2">
              {plugins.map((plugin) => (
                <div
                  key={plugin.id}
                  className="space-y-2 rounded-lg border border-border/60 px-3 py-2"
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-[13px] font-medium">
                        {plugin.id}
                        <span className="ml-2 font-normal text-muted-foreground">
                          v{plugin.currentVersion}
                        </span>
                      </p>
                      <p className="mt-0.5 text-[11px] text-muted-foreground">
                        {plugin.trusted ? "已信任" : "未信任"}
                        {plugin.requiresNodeRuntime ? " · 需要运行时" : " · 纯 UI"}
                        {" · "}
                        {plugin.runtimeState}
                        {plugin.lastError ? ` · ${plugin.lastError}` : ""}
                        {plugin.pendingVersion
                          ? ` · 待切换 v${plugin.pendingVersion}`
                          : ""}
                      </p>
                      {plugin.packageHash ? (
                        <p className="mt-1 select-all break-all font-mono text-[10px] text-muted-foreground/80">
                          hash {plugin.packageHash}
                        </p>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 items-center gap-3">
                      {!plugin.trusted ? (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          onClick={() => void trustPlugin(plugin)}
                        >
                          信任
                        </Button>
                      ) : null}
                      <Switch
                        checked={plugin.enabled}
                        disabled={busy || !plugin.trusted}
                        onCheckedChange={(enabled) => {
                          void api
                            .setPluginEnabled(plugin.id, enabled)
                            .then(refresh)
                            .catch((error) =>
                              toast.error(error instanceof Error ? error.message : String(error))
                            );
                        }}
                      />
                    </div>
                  </div>
                  {!plugin.trusted ? (
                    <p className="rounded-md bg-amber-500/10 px-2 py-1 text-[11px] text-amber-600 dark:text-amber-400">
                      信任前请确认包来源可靠：插件在信任后即可读写自身数据目录、访问网络（若含运行时）并向系统发送通知。
                    </p>
                  ) : null}
                  {plugin.trusted && plugin.mcpToolCount > 0 ? (
                    <div className="space-y-1.5 rounded-lg border border-border/50 bg-muted/30 px-2.5 py-2">
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <p className="text-[12px] font-medium">向 MCP 暴露工具</p>
                          <p className="mt-0.5 text-[11px] text-muted-foreground">
                            该插件声明了 {plugin.mcpToolCount} 个 MCP 工具；开启后 AI/Agent 可直接调用，等同于间接授予其本机能力。
                          </p>
                        </div>
                        <Switch
                          checked={plugin.mcpExposed}
                          disabled={busy}
                          onCheckedChange={(checked) => void setMcpExposed(plugin.id, checked)}
                        />
                      </div>
                      {mcpTools[plugin.id]?.length ? (
                        <p className="text-[11px] text-muted-foreground/80">
                          工具：
                          {mcpTools[plugin.id].map((tool) => tool.name).join("、")}
                        </p>
                      ) : null}
                      {plugin.mcpExposed ? (
                        <p className="rounded-md bg-amber-500/10 px-2 py-1 text-[11px] text-amber-600 dark:text-amber-400">
                          已暴露：任何可访问本机 MCP 的 AI/Agent 都能调用上述工具，请确认信任该用途。
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="flex items-center gap-2">
                    {plugin.pendingVersion ? (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={busy}
                        onClick={() => {
                          void (async () => {
                            setBusy(true);
                            try {
                              await api.trustPlugin(plugin.id, plugin.pendingVersion!, true);
                              const version = await api.promotePluginPendingVersion(plugin.id);
                              toast.success(`已切换到 v${version}`);
                              await refresh();
                            } catch (error) {
                              toast.error(
                                error instanceof Error ? error.message : String(error)
                              );
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        切换到 v{plugin.pendingVersion}
                      </Button>
                    ) : null}
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => void openDataDir(plugin.id)}
                    >
                      打开数据目录
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-destructive hover:text-destructive"
                      disabled={busy}
                      onClick={() => void uninstallPlugin(plugin.id)}
                    >
                      卸载
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
