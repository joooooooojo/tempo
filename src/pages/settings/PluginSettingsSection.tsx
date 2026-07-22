import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import type { InstalledPlugin, PluginRuntimeStatus } from "@/types";

export function PluginSettingsSection() {
  const [runtime, setRuntime] = useState<PluginRuntimeStatus | null>(null);
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [nextRuntime, nextPlugins] = await Promise.all([
      api.getPluginRuntimeStatus(),
      api.listPlugins(),
    ]);
    setRuntime(nextRuntime);
    setPlugins(nextPlugins);
  }, []);

  useEffect(() => {
    refresh().catch((error) => {
      console.error(error);
    });
  }, [refresh]);

  const installRuntime = async () => {
    setBusy(true);
    try {
      const next = await api.installPluginRuntime();
      setRuntime(next);
      toast.success(next.installed ? "插件运行时已安装" : next.message);
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
      toast.success("已卸载插件运行时");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const importPlugin = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择插件目录（含 manifest.json）",
    });
    if (!selected || Array.isArray(selected)) return;
    setBusy(true);
    try {
      const installed = await api.importLocalPlugin(selected);
      toast.success(`已导入 ${installed.pluginId}@${installed.version}（尚未信任）`);
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
            <div>
              <p className="text-[14px] font-medium">插件运行时（Node）</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                {runtime?.message ?? "正在检测…"}
              </p>
              {runtime?.installed ? (
                <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                  v{runtime.version} · {runtime.nodePath}
                </p>
              ) : (
                <p className="mt-1 text-[11px] text-muted-foreground">
                  与系统 Node 无关；仅在使用含 main 的第三方插件时需要安装。
                </p>
              )}
            </div>
            <div className="flex shrink-0 gap-2">
              {runtime?.installed ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void uninstallRuntime()}
                >
                  卸载
                </Button>
              ) : (
                <Button size="sm" disabled={busy} onClick={() => void installRuntime()}>
                  安装
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
                导入本地目录后需先信任，再启用。
              </p>
            </div>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => void importPlugin()}>
              导入本地插件
            </Button>
          </div>

          {plugins.length === 0 ? (
            <p className="text-[12px] text-muted-foreground">暂无插件</p>
          ) : (
            <div className="space-y-2">
              {plugins.map((plugin) => (
                <div
                  key={plugin.id}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2"
                >
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
                    </p>
                  </div>
                  <div className="flex items-center gap-3">
                    {!plugin.trusted ? (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={busy}
                        onClick={() => {
                          void api
                            .trustPlugin(plugin.id, plugin.currentVersion, true)
                            .then(refresh)
                            .then(() => toast.success("已信任该插件包"))
                            .catch((error) =>
                              toast.error(
                                error instanceof Error ? error.message : String(error)
                              )
                            );
                        }}
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
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
