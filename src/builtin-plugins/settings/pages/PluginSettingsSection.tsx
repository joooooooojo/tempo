import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { toast } from "sonner";
import {
  FolderOpen,
  FolderPlus,
  FileArchive,
  MoreVertical,
  Plus,
  Puzzle,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Tag } from "@/components/ui/tag";
import { Switch } from "@/components/ui/switch";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { AppIconView, listBuiltinApps, subscribeApps } from "@/apps";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import type { BuiltinMcpStatus, InstalledPlugin } from "@/types";
import { cn } from "@/lib/utils";
import {
  PluginConfigDialog,
  type PluginConfigTarget,
} from "@/builtin-plugins/settings/pages/PluginConfigDialog";
import { hasBuiltinConfigPanel } from "@/builtin-plugins/settings/pages/config-registry";

const NODE_RUNTIME_TRUST_TEXT =
  "启用此插件将允许其在本机执行代码，权限与 Tempo 相近（可读写文件、访问网络、发起进程等），请仅安装信任的来源。确定信任并继续？";
const UI_ONLY_TRUST_TEXT =
  "将在隔离视图中运行网页代码，并可调用受限的 Tempo 接口（面板控制、主题、私有存储等），不具备完整系统权限。确定信任并继续？";

/** Settings is a host shell entry, not shown in the plugin manager list. */
const HIDDEN_BUILTIN_IDS = new Set(["settings"]);

function pluginKindLabel(kind: string | undefined) {
  switch (kind) {
    case "hybrid":
      return "混合插件";
    case "headless":
      return "无界面插件";
    case "ui":
      return "UI 插件";
    case "builtin":
      return "内置";
    default:
      return null;
  }
}

function mcpStatusLabel(exposed: boolean, toolCount: number, enabledToolCount: number) {
  if (toolCount <= 0) return null;
  if (!exposed || enabledToolCount <= 0) return "MCP 已停用";
  if (enabledToolCount >= toolCount) return "MCP 已启用";
  return `MCP 部分启用 ${enabledToolCount}`;
}

export function PluginSettingsSection() {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [busy, setBusy] = useState(false);
  const [builtinMcpStatus, setBuiltinMcpStatus] = useState<Record<string, BuiltinMcpStatus>>({});
  const [fabHost, setFabHost] = useState<HTMLElement | null>(null);
  const [moreOpenId, setMoreOpenId] = useState<string | null>(null);
  const [configTarget, setConfigTarget] = useState<PluginConfigTarget | null>(null);
  const [appsRevision, setAppsRevision] = useState(0);
  const [disabledBuiltinIds, setDisabledBuiltinIds] = useState<Set<string>>(new Set());

  const builtinPlugins = useMemo(
    () => listBuiltinApps().filter((app) => !HIDDEN_BUILTIN_IDS.has(app.id)),
    [appsRevision]
  );

  useEffect(() => {
    setFabHost(document.querySelector<HTMLElement>(".settings-main"));
  }, []);

  useEffect(() => subscribeApps(() => setAppsRevision((value) => value + 1)), []);

  const refresh = useCallback(async () => {
    const [nextPlugins, settings] = await Promise.all([api.listPlugins(), api.getSettings()]);
    setPlugins(nextPlugins);
    setDisabledBuiltinIds(new Set(settings.disabled_builtin_apps ?? []));
  }, []);

  const refreshBuiltinMcpStatus = useCallback(async (appIds: string[]) => {
    const entries = await Promise.all(
      appIds.map(async (appId) => {
        try {
          const status = await api.getBuiltinMcpStatus(appId);
          return [appId, status] as const;
        } catch {
          return [appId, { exposed: false, toolCount: 0, enabledToolCount: 0 }] as const;
        }
      })
    );
    setBuiltinMcpStatus(Object.fromEntries(entries));
  }, []);

  useEffect(() => {
    refresh().catch(console.error);
  }, [refresh]);

  useEffect(() => {
    if (builtinPlugins.length === 0) return;
    void refreshBuiltinMcpStatus(builtinPlugins.map((app) => app.id));
  }, [builtinPlugins, refreshBuiltinMcpStatus]);

  const handleConfigOpenChange = useCallback((open: boolean) => {
    if (!open) setConfigTarget(null);
  }, []);

  const handlePluginMcpChanged = useCallback(() => {
    void refresh();
    if (builtinPlugins.length > 0) {
      void refreshBuiltinMcpStatus(builtinPlugins.map((app) => app.id));
    }
  }, [builtinPlugins, refresh, refreshBuiltinMcpStatus]);

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

  const openBuiltinDataDir = async (appId: string) => {
    try {
      await api.builtinOpenDataDir(appId);
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

  const promotePending = async (plugin: InstalledPlugin) => {
    if (!plugin.pendingVersion) return;
    setBusy(true);
    try {
      await api.trustPlugin(plugin.id, plugin.pendingVersion, true);
      const version = await api.promotePluginPendingVersion(plugin.id);
      toast.success(`已切换到 v${version}`);
      await refresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const uninstallPlugin = async (pluginId: string) => {
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

  const setBuiltinEnabled = async (appId: string, enabled: boolean) => {
    const previous = disabledBuiltinIds;
    const next = new Set(previous);
    if (enabled) next.delete(appId);
    else next.add(appId);
    // Optimistic + no global busy: busy used to disable every Switch (opacity flash).
    setDisabledBuiltinIds(next);
    try {
      await api.updateSettings({ disabled_builtin_apps: [...next] });
    } catch (error) {
      setDisabledBuiltinIds(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const setPluginEnabled = async (plugin: InstalledPlugin, enabled: boolean) => {
    const previous = plugins;
    setPlugins((current) =>
      current.map((item) => (item.id === plugin.id ? { ...item, enabled } : item))
    );
    try {
      await api.setPluginEnabled(plugin.id, enabled);
      await refresh();
    } catch (error) {
      setPlugins(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const hasRows = builtinPlugins.length > 0 || plugins.length > 0;
  const sortedPlugins = useMemo(
    () =>
      [...plugins].sort((a, b) => {
        const byTime = (b.installedAt || "").localeCompare(a.installedAt || "");
        if (byTime !== 0) return byTime;
        return a.id.localeCompare(b.id);
      }),
    [plugins]
  );

  return (
    <div className="settings-panel-stack plugin-settings">
      <Card>
        {!hasRows ? (
          <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">暂无插件</p>
        ) : (
          <ul className="plugin-list">
            {sortedPlugins.map((plugin) => {
              const kindLabel = pluginKindLabel(plugin.kind);
              const statusMeta = [
                plugin.pendingVersion ? `待切换 v${plugin.pendingVersion}` : null,
                plugin.lastError,
              ]
                .filter(Boolean)
                .join(" · ");
              const hasConfig =
                (plugin.settingsCount ?? 0) > 0 || plugin.mcpToolCount > 0;
              const enabled = plugin.enabled;
              const mcpLabel = mcpStatusLabel(
                plugin.mcpExposed && enabled,
                plugin.mcpToolCount,
                plugin.mcpEnabledToolCount ?? plugin.mcpToolCount
              );

              return (
                <li
                  key={plugin.id}
                  className={cn("plugin-item", !enabled && "plugin-item--disabled")}
                >
                  <span className="plugin-item__icon" aria-hidden="true">
                    {plugin.iconUrl ? (
                      <img src={plugin.iconUrl} alt="" draggable={false} />
                    ) : (
                      <Puzzle className="size-4 opacity-70" />
                    )}
                  </span>

                  <div className="plugin-item__identity" title={plugin.lastError || plugin.id}>
                    <div className="plugin-item__title-row">
                      <span className="plugin-item__name">{plugin.name || plugin.id}</span>
                      <span className="plugin-item__version">v{plugin.currentVersion}</span>
                    </div>
                    {kindLabel || mcpLabel ? (
                      <div className="plugin-item__tags">
                        {kindLabel ? <Tag value={kindLabel} size="sm" /> : null}
                        {mcpLabel ? <Tag value={mcpLabel} size="sm" /> : null}
                      </div>
                    ) : null}
                    {statusMeta ? (
                      <span className="plugin-item__meta">{statusMeta}</span>
                    ) : null}
                  </div>

                  <div className="plugin-item__controls">
                    {plugin.pendingVersion ? (
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 px-2 text-[11px]"
                        disabled={busy}
                        onClick={() => void promotePending(plugin)}
                      >
                        切换
                      </Button>
                    ) : null}

                    {!plugin.trusted ? (
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 px-2.5"
                        disabled={busy}
                        onClick={() => void trustPlugin(plugin)}
                      >
                        信任
                      </Button>
                    ) : null}

                    <Switch
                      checked={plugin.enabled}
                      disabled={!plugin.trusted}
                      onCheckedChange={(nextEnabled) => {
                        void setPluginEnabled(plugin, nextEnabled);
                      }}
                    />

                    <Popover
                      open={moreOpenId === plugin.id}
                      onOpenChange={(open) => setMoreOpenId(open ? plugin.id : null)}
                    >
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          size="icon-sm"
                          variant="ghost"
                          className="size-7 text-muted-foreground"
                          disabled={busy}
                          aria-label="更多"
                          title="更多"
                        >
                          <MoreVertical className="h-3.5 w-3.5" />
                        </Button>
                      </PopoverTrigger>
                      <PopoverContent align="end" side="bottom" className="w-52 gap-0.5 p-1">
                        <button
                          type="button"
                          className="plugin-more__item"
                          disabled={busy || !hasConfig}
                          title={hasConfig ? "插件配置" : "未声明可配置项"}
                          onClick={() => {
                            if (!hasConfig) return;
                            setMoreOpenId(null);
                            setConfigTarget({
                              source: "plugin",
                              id: plugin.id,
                              name: plugin.name || plugin.id,
                            });
                          }}
                        >
                          <SlidersHorizontal className="h-3.5 w-3.5 text-muted-foreground" />
                          插件配置
                        </button>
                        <button
                          type="button"
                          className="plugin-more__item"
                          disabled={busy}
                          onClick={() => {
                            setMoreOpenId(null);
                            void openDataDir(plugin.id);
                          }}
                        >
                          <FolderOpen className="h-3.5 w-3.5 text-muted-foreground" />
                          数据目录
                        </button>
                        <button
                          type="button"
                          className="plugin-more__item plugin-more__item--danger"
                          disabled={busy}
                          onClick={() => {
                            setMoreOpenId(null);
                            void uninstallPlugin(plugin.id);
                          }}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          卸载
                        </button>
                      </PopoverContent>
                    </Popover>
                  </div>
                </li>
              );
            })}

            {builtinPlugins.map((app) => {
              const moreId = `builtin:${app.id}`;
              const hasConfig =
                hasBuiltinConfigPanel(app.id) ||
                (builtinMcpStatus[app.id]?.toolCount ?? 0) > 0;
              const enabled = !disabledBuiltinIds.has(app.id);
              const mcpStatus = builtinMcpStatus[app.id];
              const mcpLabel = mcpStatus
                ? mcpStatusLabel(
                    mcpStatus.exposed,
                    mcpStatus.toolCount,
                    mcpStatus.enabledToolCount ?? mcpStatus.toolCount
                  )
                : null;

              return (
                <li
                  key={moreId}
                  className={cn("plugin-item", !enabled && "plugin-item--disabled")}
                >
                  <span className="plugin-item__icon" aria-hidden="true">
                    <AppIconView icon={app.icon} className="size-4 opacity-80" />
                  </span>
                  <div className="plugin-item__identity" title={app.id}>
                    <div className="plugin-item__title-row">
                      <span className="plugin-item__name">{app.name}</span>
                    </div>
                    <div className="plugin-item__tags">
                      <Tag value="内置" size="sm" />
                      {mcpLabel ? <Tag value={mcpLabel} size="sm" /> : null}
                    </div>
                  </div>
                  <div className="plugin-item__controls">
                    <Switch
                      checked={enabled}
                      onCheckedChange={(nextEnabled) =>
                        void setBuiltinEnabled(app.id, nextEnabled)
                      }
                    />
                    <Popover
                      open={moreOpenId === moreId}
                      onOpenChange={(open) => setMoreOpenId(open ? moreId : null)}
                    >
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          size="icon-sm"
                          variant="ghost"
                          className="size-7 text-muted-foreground"
                          disabled={busy}
                          aria-label="更多"
                          title="更多"
                        >
                          <MoreVertical className="h-3.5 w-3.5" />
                        </Button>
                      </PopoverTrigger>
                      <PopoverContent align="end" side="bottom" className="w-52 gap-0.5 p-1">
                        <button
                          type="button"
                          className="plugin-more__item"
                          disabled={busy || !hasConfig}
                          title={hasConfig ? "插件配置" : "未声明可配置项"}
                          onClick={() => {
                            if (!hasConfig) return;
                            setMoreOpenId(null);
                            setConfigTarget({
                              source: "builtin",
                              id: app.id,
                              name: app.name,
                            });
                          }}
                        >
                          <SlidersHorizontal className="h-3.5 w-3.5 text-muted-foreground" />
                          插件配置
                        </button>
                        <button
                          type="button"
                          className="plugin-more__item"
                          disabled={busy}
                          onClick={() => {
                            setMoreOpenId(null);
                            void openBuiltinDataDir(app.id);
                          }}
                        >
                          <FolderOpen className="h-3.5 w-3.5 text-muted-foreground" />
                          数据目录
                        </button>
                        <button
                          type="button"
                          className="plugin-more__item plugin-more__item--danger"
                          disabled
                          title="内置应用不可卸载"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          卸载
                        </button>
                      </PopoverContent>
                    </Popover>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </Card>

      {fabHost
        ? createPortal(
            <div className="plugin-install-fab">
              <div className="plugin-install-fab__anchor">
                <Popover>
                  <PopoverTrigger asChild openOnHover delay={80} closeDelay={160} disabled={busy}>
                    <button
                      type="button"
                      className={cn("plugin-install-fab__button", busy && "is-disabled")}
                      disabled={busy}
                      aria-label="安装插件"
                      title="安装插件"
                    >
                      <Plus className="plugin-install-fab__icon" aria-hidden="true" />
                    </button>
                  </PopoverTrigger>
                  <PopoverContent
                    side="top"
                    align="end"
                    sideOffset={10}
                    initialFocus={false}
                    className="w-44 gap-1 p-1.5"
                  >
                    <button
                      type="button"
                      className="plugin-install-fab__menu-item"
                      disabled={busy}
                      onClick={() => void importPluginFrom("directory")}
                    >
                      <FolderPlus className="size-3.5 shrink-0" aria-hidden="true" />
                      导入目录
                    </button>
                    <button
                      type="button"
                      className="plugin-install-fab__menu-item"
                      disabled={busy}
                      onClick={() => void importPluginFrom("zip")}
                    >
                      <FileArchive className="size-3.5 shrink-0" aria-hidden="true" />
                      导入 .zip
                    </button>
                  </PopoverContent>
                </Popover>
              </div>
            </div>,
            fabHost
          )
        : null}

      <PluginConfigDialog
        target={configTarget}
        busy={busy}
        onBusyChange={setBusy}
        onOpenChange={handleConfigOpenChange}
        onPluginMcpChanged={handlePluginMcpChanged}
      />
    </div>
  );
}
