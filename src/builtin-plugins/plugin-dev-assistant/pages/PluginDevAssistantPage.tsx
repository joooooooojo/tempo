import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Braces,
  Cable,
  FileJson2,
  FolderOpen,
  Link2,
  Plus,
  TerminalSquare,
  Unplug,
} from "lucide-react";
import { toast } from "sonner";
import { useOptionalAppNavigation } from "@/apps/navigation";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import {
  cloneManifest,
  parseEditableManifest,
  resolvedManifestKind,
  stringifyManifest,
  type EditablePluginManifest,
  type PluginKind,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import { ConnectionWorkspace } from "@/builtin-plugins/plugin-dev-assistant/pages/connection-workspace";
import { ManifestWorkspace } from "@/builtin-plugins/plugin-dev-assistant/pages/manifest-workspace";
import {
  KIND_ITEMS,
  connectionBadge,
  messageOf,
  normalizePreferences,
  projectKindLabel,
  projectMarkColor,
  projectMonogram,
  type ManifestMode,
  type WorkspaceTab,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import { TestWorkspace } from "@/builtin-plugins/plugin-dev-assistant/pages/test-workspace";
import type {
  PluginDevLogEvent,
  PluginDevPreferences,
  PluginDevProject,
  PluginDevProjectDetail,
} from "@/types";

export function PluginDevAssistantPage() {
  const navigation = useOptionalAppNavigation();
  const [projects, setProjects] = useState<PluginDevProject[]>([]);
  const [detail, setDetail] = useState<PluginDevProjectDetail | null>(null);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTab>("manifest");
  const [manifestMode, setManifestMode] = useState<ManifestMode>("visual");
  const [manifestRaw, setManifestRaw] = useState("");
  const [preferences, setPreferences] = useState<PluginDevPreferences | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createPath, setCreatePath] = useState("");
  const [createId, setCreateId] = useState("com.example.plugin");
  const [createName, setCreateName] = useState("Tempo Plugin");
  const [createKind, setCreateKind] = useState<PluginKind>("ui");
  const [logs, setLogs] = useState<PluginDevLogEvent[]>([]);
  const [commandId, setCommandId] = useState<string | null>(null);
  const [commandParams, setCommandParams] = useState("{}");
  const [commandResult, setCommandResult] = useState<string | null>(null);

  const applyDetail = useCallback((next: PluginDevProjectDetail) => {
    setDetail(next);
    setActiveProjectId(next.project.id);
    setManifestRaw(next.manifest.raw);
    setPreferences(normalizePreferences(next.preferences));
    setProjects((current) => {
      const without = current.filter(
        (project) => project.id !== next.project.id,
      );
      return [next.project, ...without];
    });
  }, []);

  const loadProject = useCallback(
    async (projectId: string) => {
      setLoading(true);
      try {
        const next = await api.getPluginDevProject(projectId);
        applyDetail(next);
        setLoadError(null);
      } catch (error) {
        setLoadError(messageOf(error));
      } finally {
        setLoading(false);
      }
    },
    [applyDetail],
  );

  const loadProjects = useCallback(async () => {
    setLoading(true);
    try {
      const next = await api.listPluginDevProjects();
      setProjects(next);
      setLoadError(null);
      const selected = activeProjectId
        ? next.find((project) => project.id === activeProjectId)
        : next[0];
      if (selected) await loadProject(selected.id);
      else setDetail(null);
    } catch (error) {
      setLoadError(messageOf(error));
    } finally {
      setLoading(false);
    }
  }, [activeProjectId, loadProject]);

  useEffect(() => {
    void loadProjects();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const unlisten = listen<PluginDevLogEvent>("plugin-dev://log", (event) => {
      if (event.payload.pluginId !== detail?.project.pluginId) return;
      setLogs((current) => [...current.slice(-199), event.payload]);
    });
    return () => void unlisten.then((dispose) => dispose());
  }, [detail?.project.pluginId]);

  useEffect(() => {
    const unlisten = listen<{
      pluginId: string;
      state: string;
      message?: string | null;
    }>("plugin-dev://runtime-state", (event) => {
      if (
        event.payload.pluginId !== detail?.project.pluginId ||
        !activeProjectId
      )
        return;
      setLogs((current) => [
        ...current.slice(-199),
        {
          pluginId: event.payload.pluginId,
          source: "host",
          message: event.payload.message
            ? `${event.payload.state}: ${event.payload.message}`
            : event.payload.state,
          at: new Date().toISOString(),
        },
      ]);
      if (event.payload.state === "ready" || event.payload.state === "failed") {
        void loadProject(activeProjectId);
      }
    });
    return () => void unlisten.then((dispose) => dispose());
  }, [activeProjectId, detail?.project.pluginId, loadProject]);

  const manifest = useMemo(
    () => parseEditableManifest(manifestRaw),
    [manifestRaw],
  );
  const kind = resolvedManifestKind(manifest);
  const commands = manifest?.contributes.commands ?? [];
  const firstApp = manifest?.contributes.apps[0];

  useEffect(() => {
    if (!commands.length) {
      setCommandId(null);
      return;
    }
    if (!commandId || !commands.some((command) => command.id === commandId)) {
      setCommandId(commands[0].id);
    }
  }, [commandId, commands]);

  const chooseDirectory = async (title: string) => {
    const path = await openNativeFileDialog({
      directory: true,
      multiple: false,
      title,
    });
    return typeof path === "string" ? path : null;
  };

  const openProject = async () => {
    const path = await chooseDirectory("打开 Tempo 插件项目");
    if (!path) return;
    setBusy(true);
    try {
      const next = await api.openPluginDevProject(path);
      applyDetail(next);
      setWorkspaceTab("manifest");
      toast.success("项目已打开");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const createProject = async () => {
    if (!createPath.trim() || !createId.trim() || !createName.trim()) return;
    setBusy(true);
    try {
      const next = await api.createPluginDevProject({
        rootPath: createPath.trim(),
        pluginId: createId.trim(),
        name: createName.trim(),
        kind: createKind,
      });
      applyDetail(next);
      setCreateOpen(false);
      setWorkspaceTab("manifest");
      toast.success("项目已创建");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const saveManifest = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      const next = await api.writePluginDevManifest(
        detail.project.id,
        manifestRaw,
        detail.manifest.hash,
      );
      applyDetail(next);
      toast.success(
        next.manifest.valid ? "Manifest 已保存" : "Manifest 已保存，请修复诊断",
      );
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const updateManifest = (mutate: (next: EditablePluginManifest) => void) => {
    if (!manifest) return;
    const next = cloneManifest(manifest);
    mutate(next);
    setManifestRaw(stringifyManifest(next));
  };

  const savePreferences = async (showToast = true) => {
    if (!detail || !preferences) return null;
    const next = await api.updatePluginDevPreferences(
      detail.project.id,
      preferences,
    );
    applyDetail(next);
    if (showToast) toast.success("连接设置已保存");
    return next;
  };

  const connect = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      await savePreferences(false);
      const status = await api.connectPluginDevProject(detail.project.id);
      const next = await api.getPluginDevProject(detail.project.id);
      applyDetail(next);
      if (status.state === "partial" || status.state === "failed") {
        toast.error(status.message ?? "插件只有部分端点连接成功");
      } else {
        toast.success("已连接到 Tempo");
      }
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      await api.disconnectPluginDevProject(detail.project.id);
      const next = await api.getPluginDevProject(detail.project.id);
      applyDetail(next);
      toast.success("开发连接已断开");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const forgetProject = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      await api.forgetPluginDevProject(detail.project.id);
      setActiveProjectId(null);
      setDetail(null);
      await loadProjects();
      toast.success("已从最近项目中移除");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const runCommand = async () => {
    if (!detail?.connection.connected || !detail.project.pluginId || !commandId)
      return;
    setBusy(true);
    setCommandResult(null);
    try {
      const params = JSON.parse(commandParams) as unknown;
      const result = await api.pluginCallCommand(
        detail.project.pluginId,
        commandId,
        params,
      );
      setCommandResult(JSON.stringify(result, null, 2));
    } catch (error) {
      setCommandResult(JSON.stringify({ error: messageOf(error) }, null, 2));
    } finally {
      setBusy(false);
    }
  };

  const openConnectedApp = () => {
    if (!detail?.connection.connected || !detail.project.pluginId || !firstApp)
      return;
    navigation?.openApp(`${detail.project.pluginId}/${firstApp.id}`);
  };

  return (
    <div className="plugin-dev-shell">
      <aside className="plugin-dev-sidebar" aria-label="插件开发项目">
        <div className="plugin-dev-sidebar__header">
          <div className="plugin-dev-brand">
            <Braces aria-hidden="true" />
            <div>
              <strong>插件开发助手</strong>
              <span>开发连接器</span>
            </div>
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              className="flex-1"
              onClick={() => setCreateOpen(true)}
            >
              <Plus data-icon="inline-start" />
              创建
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="flex-1"
              onClick={() => void openProject()}
            >
              <FolderOpen data-icon="inline-start" />
              打开
            </Button>
          </div>
        </div>
        <div className="plugin-dev-projects">
          {projects.map((project) => (
            <button
              key={project.id}
              type="button"
              className={cn(
                "plugin-dev-project",
                activeProjectId === project.id && "plugin-dev-project--active",
              )}
              onClick={() => void loadProject(project.id)}
            >
              <span
                className="plugin-dev-project__mark"
                aria-hidden="true"
                style={
                  {
                    "--plugin-dev-project-mark": projectMarkColor(
                      project.rootPath,
                    ),
                  } as CSSProperties
                }
              >
                {projectMonogram(project.rootPath)}
              </span>
              <span className="min-w-0 flex-1">
                <strong>
                  {project.name || project.pluginId || "未命名项目"}
                </strong>
                <small>{projectKindLabel(project.kind)}</small>
              </span>
            </button>
          ))}
        </div>
        {projects.length === 0 && !loading ? (
          <p className="plugin-dev-sidebar__empty">
            创建或打开包含根 Manifest 的目录
          </p>
        ) : null}
      </aside>

      <main className="plugin-dev-main">
        {loading && !detail ? (
          <div className="plugin-dev-loading">
            <Spinner />
            正在读取项目
          </div>
        ) : detail ? (
          <>
            <header className="plugin-dev-header">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <h1>{detail.project.name || detail.project.pluginId}</h1>
                  {connectionBadge(detail.connection)}
                  <Badge variant="outline">{projectKindLabel(kind)}</Badge>
                </div>
                <p title={detail.project.rootPath}>{detail.project.rootPath}</p>
              </div>
              <div className="flex shrink-0 gap-2">
                {detail.connection.connected ? (
                  <Button
                    variant="outline"
                    onClick={() => void disconnect()}
                    disabled={busy}
                  >
                    <Unplug data-icon="inline-start" />
                    断开
                  </Button>
                ) : (
                  <Button
                    onClick={() => void connect()}
                    disabled={busy || !detail.manifest.valid}
                  >
                    {busy ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <Cable data-icon="inline-start" />
                    )}
                    连接到 Tempo
                  </Button>
                )}
              </div>
            </header>

            <Tabs
              value={workspaceTab}
              onValueChange={(value) => setWorkspaceTab(value as WorkspaceTab)}
              className="plugin-dev-workspace"
            >
              <div className="plugin-dev-tabs-bar">
                <TabsList className="plugin-dev-tabs">
                  <TabsTrigger value="manifest">
                    <FileJson2 data-icon="inline-start" />
                    Manifest
                  </TabsTrigger>
                  <TabsTrigger value="connection">
                    <Link2 data-icon="inline-start" />
                    开发连接
                  </TabsTrigger>
                  <TabsTrigger value="test" disabled={commands.length === 0}>
                    <TerminalSquare data-icon="inline-start" />
                    测试
                  </TabsTrigger>
                </TabsList>
              </div>

              <TabsContent value="manifest" className="plugin-dev-content">
                <ManifestWorkspace
                  detail={detail}
                  manifest={manifest}
                  raw={manifestRaw}
                  mode={manifestMode}
                  busy={busy}
                  onModeChange={setManifestMode}
                  onRawChange={setManifestRaw}
                  onUpdate={updateManifest}
                  onSave={() => void saveManifest()}
                  onForget={() => void forgetProject()}
                />
              </TabsContent>
              <TabsContent value="connection" className="plugin-dev-content">
                {preferences ? (
                  <ConnectionWorkspace
                    detail={detail}
                    kind={kind}
                    preferences={preferences}
                    logs={logs}
                    busy={busy}
                    onPreferencesChange={setPreferences}
                    onChooseDirectory={chooseDirectory}
                    onSave={() => void savePreferences()}
                    onConnect={() => void connect()}
                    onDisconnect={() => void disconnect()}
                    onReconnectRuntime={async () => {
                      setBusy(true);
                      try {
                        await api.reconnectPluginDevRuntime(detail.project.id);
                        applyDetail(
                          await api.getPluginDevProject(detail.project.id),
                        );
                        toast.success("Runtime 已重新连接");
                      } catch (error) {
                        toast.error(messageOf(error));
                      } finally {
                        setBusy(false);
                      }
                    }}
                    onProbe={async () => {
                      const result = await api.probePluginDevUiUrl(
                        preferences.uiServiceUrl ?? "",
                      );
                      result.reachable
                        ? toast.success(result.message)
                        : toast.error(result.message);
                    }}
                    onOpenApp={openConnectedApp}
                  />
                ) : null}
              </TabsContent>
              <TabsContent value="test" className="plugin-dev-content">
                <TestWorkspace
                  detail={detail}
                  manifest={manifest}
                  commands={commands}
                  commandId={commandId}
                  params={commandParams}
                  result={commandResult}
                  busy={busy}
                  onCommandChange={setCommandId}
                  onParamsChange={setCommandParams}
                  onRun={() => void runCommand()}
                />
              </TabsContent>
            </Tabs>
          </>
        ) : (
          <Empty className="h-full rounded-none border-0">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Braces />
              </EmptyMedia>
              <EmptyTitle>选择一个插件项目</EmptyTitle>
              <EmptyDescription>
                {loadError ?? "项目根目录中的 manifest.json 是开发连接的入口。"}
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button onClick={() => setCreateOpen(true)}>
                <Plus data-icon="inline-start" />
                创建项目
              </Button>
              <Button variant="outline" onClick={() => void openProject()}>
                <FolderOpen data-icon="inline-start" />
                打开项目
              </Button>
            </EmptyContent>
          </Empty>
        )}
      </main>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogPanel className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>创建插件项目</DialogTitle>
            <DialogDescription>
              只在所选根目录创建 manifest.json。
            </DialogDescription>
          </DialogHeader>
          <DialogContent>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="plugin-dev-create-path">
                  项目根目录
                </FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="plugin-dev-create-path"
                    value={createPath}
                    onChange={(event) => setCreatePath(event.target.value)}
                    placeholder="C:\\projects\\my-plugin"
                  />
                  <Button
                    variant="outline"
                    size="icon"
                    aria-label="选择目录"
                    onClick={async () => {
                      const path = await chooseDirectory("选择插件项目根目录");
                      if (path) setCreatePath(path);
                    }}
                  >
                    <FolderOpen />
                  </Button>
                </div>
              </Field>
              <div className="grid grid-cols-2 gap-4">
                <Field>
                  <FieldLabel htmlFor="plugin-dev-create-id">
                    插件 ID
                  </FieldLabel>
                  <Input
                    id="plugin-dev-create-id"
                    value={createId}
                    onChange={(event) => setCreateId(event.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="plugin-dev-create-name">名称</FieldLabel>
                  <Input
                    id="plugin-dev-create-name"
                    value={createName}
                    onChange={(event) => setCreateName(event.target.value)}
                  />
                </Field>
              </div>
              <Field>
                <FieldLabel>类型</FieldLabel>
                <Select
                  items={KIND_ITEMS}
                  value={createKind}
                  onValueChange={(value) =>
                    value && setCreateKind(value as PluginKind)
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {KIND_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            </FieldGroup>
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              取消
            </Button>
            <Button
              onClick={() => void createProject()}
              disabled={
                busy ||
                !createPath.trim() ||
                !createId.trim() ||
                !createName.trim()
              }
            >
              {busy ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <Plus data-icon="inline-start" />
              )}
              创建项目
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </div>
  );
}

