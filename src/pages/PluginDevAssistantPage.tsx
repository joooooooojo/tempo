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
  Check,
  Code2,
  FileJson2,
  Folder,
  FolderOpen,
  Link2,
  Play,
  Plus,
  RefreshCw,
  Save,
  ServerCog,
  Square,
  TerminalSquare,
  Trash2,
  TriangleAlert,
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
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
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
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import {
  cloneManifest,
  parseEditableManifest,
  resolvedManifestKind,
  stringifyManifest,
  type EditablePluginAction,
  type EditablePluginApp,
  type EditablePluginCommand,
  type EditablePluginHook,
  type EditablePluginManifest,
  type EditablePluginMcpTool,
  type EditablePluginSetting,
  type PluginKind,
} from "@/pages/plugin-dev/manifest";
import type {
  PluginDevConnectionStatus,
  PluginDevLogEvent,
  PluginDevPreferences,
  PluginDevProject,
  PluginDevProjectDetail,
} from "@/types";

type WorkspaceTab = "manifest" | "connection" | "test";
type ManifestMode = "visual" | "json";

const KIND_ITEMS = [
  { value: "ui", label: "UI" },
  { value: "headless", label: "Headless" },
  { value: "hybrid", label: "Hybrid" },
] as const;
const UI_SOURCE_ITEMS = [
  { value: "url", label: "服务 URL" },
  { value: "static", label: "静态目录" },
] as const;
const WINDOW_MODE_ITEMS = [
  { value: "normal", label: "主面板" },
  { value: "standalone", label: "独立窗口" },
] as const;
const TARGET_ITEMS = [
  { value: "app", label: "打开 App" },
  { value: "command", label: "调用 Command" },
] as const;
const SETTING_TYPE_ITEMS = [
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

function projectFolderName(rootPath: string): string {
  const normalized = rootPath.replace(/[\\/]+$/, "");
  const segments = normalized.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? normalized;
}

function projectMonogram(rootPath: string): string {
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

function projectMarkColor(rootPath: string): string {
  let hash = 0;
  for (const character of rootPath.toLocaleLowerCase()) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }
  return PROJECT_MARK_COLORS[hash % PROJECT_MARK_COLORS.length];
}

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function connectionBadge(status: PluginDevConnectionStatus) {
  if (!status.connected) return <Badge variant="outline">未连接</Badge>;
  if (status.state === "failed")
    return <Badge variant="destructive">连接失败</Badge>;
  if (status.state === "partial")
    return <Badge variant="secondary">部分连接</Badge>;
  return <Badge>已连接</Badge>;
}

function projectKindLabel(kind?: string | null) {
  if (kind === "headless") return "Headless";
  if (kind === "hybrid") return "Hybrid";
  return "UI";
}

function normalizePreferences(
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

function ManifestWorkspace({
  detail,
  manifest,
  raw,
  mode,
  busy,
  onModeChange,
  onRawChange,
  onUpdate,
  onSave,
  onForget,
}: {
  detail: PluginDevProjectDetail;
  manifest: EditablePluginManifest | null;
  raw: string;
  mode: ManifestMode;
  busy: boolean;
  onModeChange: (mode: ManifestMode) => void;
  onRawChange: (raw: string) => void;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
  onSave: () => void;
  onForget: () => void;
}) {
  const diagnostics = detail.manifest.diagnostics;
  return (
    <div className="plugin-dev-panel">
      <div className="plugin-dev-panel__toolbar">
        <Tabs
          value={mode}
          onValueChange={(value) => onModeChange(value as ManifestMode)}
        >
          <TabsList>
            <TabsTrigger value="visual">
              <Braces data-icon="inline-start" />
              可视化
            </TabsTrigger>
            <TabsTrigger value="json">
              <Code2 data-icon="inline-start" />
              JSON
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <div className="flex gap-2">
          <Button variant="ghost" onClick={onForget}>
            <Trash2 data-icon="inline-start" />
            移除记录
          </Button>
          <Button onClick={onSave} disabled={busy}>
            {busy ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <Save data-icon="inline-start" />
            )}
            保存 Manifest
          </Button>
        </div>
      </div>

      {diagnostics.length > 0 ? (
        <div className="plugin-dev-diagnostics" role="alert">
          <TriangleAlert aria-hidden="true" />
          <div>
            {diagnostics.map((diagnostic) => (
              <p key={`${diagnostic.code}-${diagnostic.line ?? 0}`}>
                {diagnostic.line
                  ? `${diagnostic.line}:${diagnostic.column ?? 1} `
                  : ""}
                {diagnostic.message}
              </p>
            ))}
          </div>
        </div>
      ) : (
        <div className="plugin-dev-valid">
          <Check aria-hidden="true" />
          Manifest 校验通过
        </div>
      )}

      {mode === "json" ? (
        <Textarea
          className="plugin-dev-json-editor"
          value={raw}
          onChange={(event) => onRawChange(event.target.value)}
          spellCheck={false}
          aria-label="manifest.json"
        />
      ) : manifest ? (
        <div className="plugin-dev-form">
          <FieldSet>
            <FieldLegend>基础信息</FieldLegend>
            <FieldGroup>
              <div className="grid grid-cols-2 gap-4">
                <Field>
                  <FieldLabel htmlFor="manifest-id">插件 ID</FieldLabel>
                  <Input
                    id="manifest-id"
                    value={manifest.id ?? ""}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.id = event.target.value;
                      })
                    }
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="manifest-name">名称</FieldLabel>
                  <Input
                    id="manifest-name"
                    value={manifest.name ?? ""}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.name = event.target.value;
                      })
                    }
                  />
                </Field>
              </div>
              <div className="grid grid-cols-3 gap-4">
                <Field>
                  <FieldLabel htmlFor="manifest-version">版本</FieldLabel>
                  <Input
                    id="manifest-version"
                    value={manifest.version ?? ""}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.version = event.target.value;
                      })
                    }
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="manifest-tempo">Tempo</FieldLabel>
                  <Input
                    id="manifest-tempo"
                    value={manifest.engines.tempo ?? ""}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.engines.tempo = event.target.value;
                      })
                    }
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="manifest-api">Plugin API</FieldLabel>
                  <Input
                    id="manifest-api"
                    value={manifest.engines.pluginApi ?? ""}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.engines.pluginApi = event.target.value;
                      })
                    }
                  />
                </Field>
              </div>
              <Field>
                <FieldLabel>类型</FieldLabel>
                <Select
                  items={KIND_ITEMS}
                  value={resolvedManifestKind(manifest)}
                  onValueChange={(value) =>
                    value &&
                    onUpdate((next) => {
                      const nextKind = value as PluginKind;
                      next.kind = nextKind;
                      if (nextKind === "ui") {
                        delete next.main;
                        if (next.contributes.apps.length === 0) {
                          next.contributes.apps.push({
                            id: "main",
                            name: next.name || "Main",
                            entry: "index.html",
                            keywords: [],
                            windowMode: "normal",
                          });
                        }
                      } else if (nextKind === "headless") {
                        next.main = next.main || "main.mjs";
                        next.contributes.apps = [];
                      } else {
                        next.main = next.main || "main.mjs";
                        if (next.contributes.apps.length === 0) {
                          next.contributes.apps.push({
                            id: "main",
                            name: next.name || "Main",
                            entry: "index.html",
                            keywords: [],
                            windowMode: "normal",
                          });
                        }
                      }
                    })
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
                <FieldDescription>
                  类型由 Apps 和 Runtime 入口共同决定。
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="manifest-description">描述</FieldLabel>
                <Textarea
                  id="manifest-description"
                  rows={3}
                  value={manifest.description ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.description = event.target.value;
                    })
                  }
                />
              </Field>
              {manifest.main !== undefined ? (
                <Field>
                  <FieldLabel htmlFor="manifest-main">
                    正式 Runtime 入口
                  </FieldLabel>
                  <Input
                    id="manifest-main"
                    value={manifest.main}
                    onChange={(event) =>
                      onUpdate((next) => {
                        next.main = event.target.value;
                      })
                    }
                  />
                  <FieldDescription>
                    开发入口在“开发连接”中单独设置。
                  </FieldDescription>
                </Field>
              ) : null}
            </FieldGroup>
          </FieldSet>
          <Separator />
          <ContributionSection
            title="Apps"
            description="平台中可打开的插件页面"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.apps.push({
                  id: `app-${next.contributes.apps.length + 1}`,
                  name: "New App",
                  entry: "index.html",
                  keywords: [],
                  windowMode: "normal",
                }),
              )
            }
          >
            {manifest.contributes.apps.map((app, index) => (
              <AppEditor
                key={`${app.id}-${index}`}
                app={app}
                index={index}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
          <Separator />
          <ContributionSection
            title="Commands"
            description="由 Runtime 注册并通过真实 RPC 调用"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.commands.push({
                  id: `command-${next.contributes.commands.length + 1}`,
                  title: "New Command",
                  visibility: "private",
                }),
              )
            }
          >
            {manifest.contributes.commands.map((command, index) => (
              <CommandEditor
                key={`${command.id}-${index}`}
                command={command}
                index={index}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
          <Separator />
          <ContributionSection
            title="Actions"
            description="将主面板输入路由到 App 或 Command"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.actions.push({
                  id: `action-${next.contributes.actions.length + 1}`,
                  name: "New Action",
                  accepts: ["text"],
                  app: next.contributes.apps[0]?.id,
                }),
              )
            }
          >
            {manifest.contributes.actions.map((action, index) => (
              <ActionEditor
                key={`${action.id}-${index}`}
                action={action}
                index={index}
                manifest={manifest}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
          <Separator />
          <ContributionSection
            title="Hooks"
            description="将平台事件路由到 Runtime Command"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.hooks.push({
                  event: "clipboard.changed",
                  command: next.contributes.commands[0]?.id ?? "",
                }),
              )
            }
          >
            {manifest.contributes.hooks.map((hook, index) => (
              <HookEditor
                key={`${hook.event}-${index}`}
                hook={hook}
                index={index}
                commands={manifest.contributes.commands}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
          <Separator />
          <ContributionSection
            title="MCP Tools"
            description="在助手内测试，开发态默认不向外暴露"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.mcpTools.push({
                  name: `tool-${next.contributes.mcpTools.length + 1}`,
                  description: "Tool description",
                  command: next.contributes.commands[0]?.id ?? "",
                  inputSchema: { type: "object", properties: {} },
                }),
              )
            }
          >
            {manifest.contributes.mcpTools.map((tool, index) => (
              <McpToolEditor
                key={`${tool.name}-${index}`}
                tool={tool}
                index={index}
                commands={manifest.contributes.commands}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
          <Separator />
          <ContributionSection
            title="Settings"
            description="由 Tempo 使用内置控件渲染的插件设置"
            onAdd={() =>
              onUpdate((next) =>
                next.contributes.settings.push({
                  id: `setting-${next.contributes.settings.length + 1}`,
                  type: "switch",
                  title: "New Setting",
                  default: false,
                }),
              )
            }
          >
            {manifest.contributes.settings.map((setting, index) => (
              <SettingEditor
                key={`${setting.id}-${index}`}
                setting={setting}
                index={index}
                onUpdate={onUpdate}
              />
            ))}
          </ContributionSection>
        </div>
      ) : (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <TriangleAlert />
            </EmptyMedia>
            <EmptyTitle>JSON 尚不可视化</EmptyTitle>
            <EmptyDescription>
              修复 JSON 语法后即可切换到可视化编辑。
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </div>
  );
}

function ContributionSection({
  title,
  description,
  onAdd,
  children,
}: {
  title: string;
  description: string;
  onAdd: () => void;
  children: React.ReactNode;
}) {
  return (
    <section className="plugin-dev-contribution">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
        <Button size="sm" variant="outline" onClick={onAdd}>
          <Plus data-icon="inline-start" />
          添加
        </Button>
      </div>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function AppEditor({
  app,
  index,
  onUpdate,
}: {
  app: EditablePluginApp;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1.4fr_1fr] gap-3">
        <Field>
          <FieldLabel>App ID</FieldLabel>
          <Input
            value={app.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.apps[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>名称</FieldLabel>
          <Input
            value={app.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.apps[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>窗口</FieldLabel>
          <Select
            items={WINDOW_MODE_ITEMS}
            value={app.windowMode ?? "normal"}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                next.contributes.apps[index].windowMode = value as
                  "normal" | "standalone";
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {WINDOW_MODE_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 App"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.apps.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function CommandEditor({
  command,
  index,
  onUpdate,
}: {
  command: EditablePluginCommand;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-2 gap-3">
        <Field>
          <FieldLabel>Command ID</FieldLabel>
          <Input
            value={command.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.commands[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>标题</FieldLabel>
          <Input
            value={command.title}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.commands[index].title = event.target.value;
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Command"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.commands.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function ActionEditor({
  action,
  index,
  manifest,
  onUpdate,
}: {
  action: EditablePluginAction;
  index: number;
  manifest: EditablePluginManifest;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const targetKind = action.command ? "command" : "app";
  const targetItems =
    targetKind === "command"
      ? manifest.contributes.commands.map((item) => ({
          value: item.id,
          label: item.title || item.id,
        }))
      : manifest.contributes.apps.map((item) => ({
          value: item.id,
          label: item.name || item.id,
        }));
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1.2fr_0.8fr_1.2fr] gap-3">
        <Field>
          <FieldLabel>Action ID</FieldLabel>
          <Input
            value={action.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.actions[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>名称</FieldLabel>
          <Input
            value={action.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.actions[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>类型</FieldLabel>
          <Select
            items={TARGET_ITEMS}
            value={targetKind}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                const target = next.contributes.actions[index];
                delete target.app;
                delete target.command;
                if (value === "command")
                  target.command = next.contributes.commands[0]?.id ?? "";
                else target.app = next.contributes.apps[0]?.id ?? "";
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {TARGET_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel>目标</FieldLabel>
          <Select
            items={targetItems}
            value={action.command ?? action.app ?? null}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                const target = next.contributes.actions[index];
                if (targetKind === "command") target.command = value;
                else target.app = value;
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {targetItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Action"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.actions.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function HookEditor({
  hook,
  index,
  commands,
  onUpdate,
}: {
  hook: EditablePluginHook;
  index: number;
  commands: EditablePluginCommand[];
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-2 gap-3">
        <Field>
          <FieldLabel>事件</FieldLabel>
          <Input
            value={hook.event}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.hooks[index].event = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>Command</FieldLabel>
          <Select
            items={commandItems}
            value={hook.command}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                next.contributes.hooks[index].command = value;
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {commandItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Hook"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.hooks.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function McpToolEditor({
  tool,
  index,
  commands,
  onUpdate,
}: {
  tool: EditablePluginMcpTool;
  index: number;
  commands: EditablePluginCommand[];
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1fr_1.5fr] gap-3">
        <Field>
          <FieldLabel>Tool 名称</FieldLabel>
          <Input
            value={tool.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.mcpTools[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>Command</FieldLabel>
          <Select
            items={commandItems}
            value={tool.command}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                next.contributes.mcpTools[index].command = value;
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {commandItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel>描述</FieldLabel>
          <Input
            value={tool.description}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.mcpTools[index].description =
                  event.target.value;
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 MCP Tool"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.mcpTools.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function SettingEditor({
  setting,
  index,
  onUpdate,
}: {
  setting: EditablePluginSetting;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_0.8fr_1.2fr_1fr] gap-3">
        <Field>
          <FieldLabel>Setting ID</FieldLabel>
          <Input
            value={setting.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.settings[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>控件</FieldLabel>
          <Select
            items={SETTING_TYPE_ITEMS}
            value={setting.type}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                const target = next.contributes.settings[index];
                target.type = value as EditablePluginSetting["type"];
                target.default =
                  value === "switch"
                    ? false
                    : value === "multiselect"
                      ? []
                      : "";
                if (value === "select" || value === "multiselect")
                  target.options = target.options?.length
                    ? target.options
                    : [{ value: "default", label: "Default" }];
                else delete target.options;
              })
            }
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {SETTING_TYPE_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel>标题</FieldLabel>
          <Input
            value={setting.title}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.settings[index].title = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>默认值</FieldLabel>
          <Input
            value={
              typeof setting.default === "string"
                ? setting.default
                : JSON.stringify(setting.default)
            }
            onChange={(event) =>
              onUpdate((next) => {
                const raw = event.target.value;
                try {
                  next.contributes.settings[index].default = JSON.parse(
                    raw,
                  ) as unknown;
                } catch {
                  next.contributes.settings[index].default = raw;
                }
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Setting"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.settings.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

function ConnectionWorkspace({
  detail,
  kind,
  preferences,
  logs,
  busy,
  onPreferencesChange,
  onChooseDirectory,
  onSave,
  onConnect,
  onDisconnect,
  onReconnectRuntime,
  onProbe,
  onOpenApp,
}: {
  detail: PluginDevProjectDetail;
  kind: PluginKind;
  preferences: PluginDevPreferences;
  logs: PluginDevLogEvent[];
  busy: boolean;
  onPreferencesChange: (next: PluginDevPreferences) => void;
  onChooseDirectory: (title: string) => Promise<string | null>;
  onSave: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onReconnectRuntime: () => void;
  onProbe: () => Promise<void>;
  onOpenApp: () => void;
}) {
  const hasUi = kind !== "headless";
  const hasRuntime = kind !== "ui";
  return (
    <div className="plugin-dev-panel">
      <div className="plugin-dev-connection-track">
        <ConnectionNode
          icon={Folder}
          title="项目根"
          value="manifest.json"
          active
        />
        <span className="plugin-dev-connection-track__line" />
        <ConnectionNode
          icon={hasUi ? ServerCog : TerminalSquare}
          title={hasUi ? "UI" : "Runtime"}
          value={
            hasUi
              ? preferences.uiSourceKind === "static"
                ? "静态目录"
                : "服务 URL"
              : "JavaScript"
          }
          active={Boolean(detail.connection.connected)}
        />
        <span className="plugin-dev-connection-track__line" />
        <ConnectionNode
          icon={Cable}
          title="Tempo"
          value={detail.connection.connected ? "已连接" : "未连接"}
          active={detail.connection.connected}
        />
      </div>
      <Separator />
      <div className="plugin-dev-form">
        {hasUi ? (
          <FieldSet>
            <FieldLegend>UI 连接</FieldLegend>
            <FieldDescription>
              连接外部服务地址或已有静态文件，不由助手启动开发服务器。
            </FieldDescription>
            <FieldGroup>
              <Field>
                <FieldLabel>来源</FieldLabel>
                <Select
                  items={UI_SOURCE_ITEMS}
                  value={preferences.uiSourceKind ?? "url"}
                  onValueChange={(value) =>
                    value &&
                    onPreferencesChange({
                      ...preferences,
                      uiSourceKind: value as "url" | "static",
                    })
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {UI_SOURCE_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {preferences.uiSourceKind === "static" ? (
                <Field>
                  <FieldLabel htmlFor="plugin-dev-static-root">
                    静态根目录
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="plugin-dev-static-root"
                      value={preferences.uiStaticRoot ?? ""}
                      placeholder={detail.project.rootPath}
                      onChange={(event) =>
                        onPreferencesChange({
                          ...preferences,
                          uiStaticRoot: event.target.value,
                        })
                      }
                    />
                    <Button
                      size="icon"
                      variant="outline"
                      aria-label="选择静态目录"
                      onClick={async () => {
                        const path =
                          await onChooseDirectory("选择 UI 静态根目录");
                        if (path)
                          onPreferencesChange({
                            ...preferences,
                            uiStaticRoot: path,
                          });
                      }}
                    >
                      <FolderOpen />
                    </Button>
                  </div>
                  <FieldDescription>留空时使用项目根目录。</FieldDescription>
                </Field>
              ) : (
                <Field>
                  <FieldLabel htmlFor="plugin-dev-service-url">
                    服务 URL
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="plugin-dev-service-url"
                      value={preferences.uiServiceUrl ?? ""}
                      placeholder="http://127.0.0.1:5173/"
                      onChange={(event) =>
                        onPreferencesChange({
                          ...preferences,
                          uiServiceUrl: event.target.value,
                        })
                      }
                    />
                    <Button variant="outline" onClick={() => void onProbe()}>
                      <RefreshCw data-icon="inline-start" />
                      检测
                    </Button>
                  </div>
                  <FieldDescription>
                    仅接受 localhost、127.0.0.1 或 [::1]。
                  </FieldDescription>
                </Field>
              )}
            </FieldGroup>
          </FieldSet>
        ) : null}
        {hasUi && hasRuntime ? <Separator /> : null}
        {hasRuntime ? (
          <FieldSet>
            <FieldLegend>Runtime 连接</FieldLegend>
            <FieldDescription>
              选择外部工具已经生成的 .mjs 或 .js 入口。
            </FieldDescription>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="plugin-dev-runtime-entry">
                  JavaScript 入口
                </FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="plugin-dev-runtime-entry"
                    value={preferences.runtimeDevEntry ?? ""}
                    placeholder={`${detail.project.rootPath}\\main.mjs`}
                    onChange={(event) =>
                      onPreferencesChange({
                        ...preferences,
                        runtimeDevEntry: event.target.value,
                      })
                    }
                  />
                  <Button
                    size="icon"
                    variant="outline"
                    aria-label="选择 Runtime 入口"
                    onClick={async () => {
                      const path = await openNativeFileDialog({
                        multiple: false,
                        directory: false,
                        title: "选择 Runtime JavaScript 入口",
                        filters: [
                          { name: "JavaScript", extensions: ["js", "mjs"] },
                        ],
                      });
                      if (typeof path === "string")
                        onPreferencesChange({
                          ...preferences,
                          runtimeDevEntry: path,
                        });
                    }}
                  >
                    <FolderOpen />
                  </Button>
                </div>
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="plugin-dev-auto-reconnect">
                  入口变化后自动重连
                </FieldLabel>
                <Switch
                  id="plugin-dev-auto-reconnect"
                  checked={preferences.autoReconnectRuntime}
                  onCheckedChange={(checked) =>
                    onPreferencesChange({
                      ...preferences,
                      autoReconnectRuntime: checked,
                    })
                  }
                />
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="plugin-dev-real-hooks">
                  接收真实 Hook 事件
                </FieldLabel>
                <Switch
                  id="plugin-dev-real-hooks"
                  checked={preferences.receiveRealHooks}
                  onCheckedChange={(checked) =>
                    onPreferencesChange({
                      ...preferences,
                      receiveRealHooks: checked,
                    })
                  }
                />
              </Field>
            </FieldGroup>
          </FieldSet>
        ) : null}
        <Separator />
        <FieldSet>
          <FieldLegend>开发数据</FieldLegend>
          <FieldDescription>
            默认隔离 Tempo 管理的 KV 和推荐 dataPath；Runtime
            自行读写文件不受此设置限制。
          </FieldDescription>
          <FieldGroup>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="plugin-dev-production-data">
                使用正式插件数据
              </FieldLabel>
              <Switch
                id="plugin-dev-production-data"
                checked={preferences.useProductionData}
                onCheckedChange={(checked) =>
                  onPreferencesChange({
                    ...preferences,
                    useProductionData: checked,
                  })
                }
              />
            </Field>
            {preferences.useProductionData ? (
              <FieldDescription>
                开发代码将读取并修改同 ID 正式插件的宿主管理数据。
              </FieldDescription>
            ) : null}
          </FieldGroup>
        </FieldSet>
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onSave} disabled={busy}>
            <Save data-icon="inline-start" />
            保存设置
          </Button>
          {detail.connection.connected && hasRuntime ? (
            <Button
              variant="outline"
              onClick={onReconnectRuntime}
              disabled={busy}
            >
              <RefreshCw data-icon="inline-start" />
              重连 Runtime
            </Button>
          ) : null}
          {detail.connection.connected && hasUi ? (
            <Button variant="outline" onClick={onOpenApp}>
              <Play data-icon="inline-start" />
              打开 App
            </Button>
          ) : null}
          {detail.connection.connected ? (
            <Button
              variant="destructive"
              onClick={onDisconnect}
              disabled={busy}
            >
              <Square data-icon="inline-start" />
              断开
            </Button>
          ) : (
            <Button onClick={onConnect} disabled={busy}>
              <Cable data-icon="inline-start" />
              连接到 Tempo
            </Button>
          )}
        </div>
        {hasRuntime ? (
          <section className="plugin-dev-log">
            <div className="plugin-dev-section-heading">
              <div>
                <h2>Runtime 日志</h2>
                <p>只显示当前开发 Runtime 的 stdout/stderr。</p>
              </div>
              <Badge variant="outline">{logs.length} 行</Badge>
            </div>
            <pre>
              {logs.length
                ? logs
                    .map((entry) => `[${entry.source}] ${entry.message}`)
                    .join("\n")
                : "等待 Runtime 输出..."}
            </pre>
          </section>
        ) : null}
      </div>
    </div>
  );
}

function ConnectionNode({
  icon: Icon,
  title,
  value,
  active,
}: {
  icon: React.ComponentType<React.SVGProps<SVGSVGElement>>;
  title: string;
  value: string;
  active: boolean;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-connection-node",
        active && "plugin-dev-connection-node--active",
      )}
    >
      <Icon aria-hidden="true" />
      <span>
        <strong>{title}</strong>
        <small>{value}</small>
      </span>
    </div>
  );
}

function TestWorkspace({
  detail,
  manifest,
  commands,
  commandId,
  params,
  result,
  busy,
  onCommandChange,
  onParamsChange,
  onRun,
}: {
  detail: PluginDevProjectDetail;
  manifest: EditablePluginManifest | null;
  commands: EditablePluginCommand[];
  commandId: string | null;
  params: string;
  result: string | null;
  busy: boolean;
  onCommandChange: (value: string | null) => void;
  onParamsChange: (value: string) => void;
  onRun: () => void;
}) {
  const [mode, setMode] = useState<"command" | "hook" | "mcp">("command");
  const [hookEvent, setHookEvent] = useState<string | null>(
    manifest?.contributes.hooks[0]?.event ?? null,
  );
  const [hookPayload, setHookPayload] = useState("{}");
  const [hookResult, setHookResult] = useState<string | null>(null);
  const [mcpToolName, setMcpToolName] = useState<string | null>(
    manifest?.contributes.mcpTools[0]?.name ?? null,
  );
  const [mcpArguments, setMcpArguments] = useState("{}");
  const [mcpResult, setMcpResult] = useState<string | null>(null);
  const [localBusy, setLocalBusy] = useState(false);
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  const hookItems = (manifest?.contributes.hooks ?? []).map((hook) => ({
    value: hook.event,
    label: `${hook.event} -> ${hook.command}`,
  }));
  const mcpItems = (manifest?.contributes.mcpTools ?? []).map((tool) => ({
    value: tool.name,
    label: tool.name,
  }));

  useEffect(() => {
    if (!hookEvent || !hookItems.some((item) => item.value === hookEvent)) {
      setHookEvent(hookItems[0]?.value ?? null);
    }
    if (!mcpToolName || !mcpItems.some((item) => item.value === mcpToolName)) {
      setMcpToolName(mcpItems[0]?.value ?? null);
    }
  }, [hookEvent, hookItems, mcpItems, mcpToolName]);

  const runHook = async () => {
    if (!hookEvent) return;
    setLocalBusy(true);
    try {
      const payload = JSON.parse(hookPayload) as unknown;
      const next = await api.simulatePluginDevHook(
        detail.project.id,
        hookEvent,
        payload,
      );
      setHookResult(JSON.stringify(next, null, 2));
    } catch (error) {
      setHookResult(JSON.stringify({ error: messageOf(error) }, null, 2));
    } finally {
      setLocalBusy(false);
    }
  };

  const runMcpTool = async () => {
    if (!mcpToolName) return;
    setLocalBusy(true);
    try {
      const argumentsValue = JSON.parse(mcpArguments) as unknown;
      const next = await api.runPluginDevMcpTool(
        detail.project.id,
        mcpToolName,
        argumentsValue,
      );
      setMcpResult(JSON.stringify(next, null, 2));
    } catch (error) {
      setMcpResult(JSON.stringify({ error: messageOf(error) }, null, 2));
    } finally {
      setLocalBusy(false);
    }
  };

  return (
    <div className="plugin-dev-panel">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>Headless 测试</h2>
          <p>所有请求通过现有 Supervisor 和平台校验链路。</p>
        </div>
        {connectionBadge(detail.connection)}
      </div>
      <Tabs
        value={mode}
        onValueChange={(value) => setMode(value as typeof mode)}
      >
        <TabsList>
          <TabsTrigger value="command" disabled={commandItems.length === 0}>
            Command
          </TabsTrigger>
          <TabsTrigger value="hook" disabled={hookItems.length === 0}>
            Hook
          </TabsTrigger>
          <TabsTrigger value="mcp" disabled={mcpItems.length === 0}>
            MCP Tool
          </TabsTrigger>
        </TabsList>
        <TabsContent value="command">
          <JsonTestPanel
            label="Command"
            items={commandItems}
            value={commandId}
            inputId="plugin-dev-command-params"
            inputLabel="参数 JSON"
            input={params}
            result={result}
            actionLabel="运行 Command"
            busy={busy}
            enabled={detail.connection.connected && Boolean(commandId)}
            onValueChange={onCommandChange}
            onInputChange={onParamsChange}
            onRun={onRun}
          />
        </TabsContent>
        <TabsContent value="hook">
          <JsonTestPanel
            label="Hook 事件"
            items={hookItems}
            value={hookEvent}
            inputId="plugin-dev-hook-payload"
            inputLabel="事件 Payload"
            input={hookPayload}
            result={hookResult}
            actionLabel="模拟 Hook"
            busy={localBusy}
            enabled={detail.connection.connected && Boolean(hookEvent)}
            onValueChange={setHookEvent}
            onInputChange={setHookPayload}
            onRun={() => void runHook()}
          />
        </TabsContent>
        <TabsContent value="mcp">
          <JsonTestPanel
            label="MCP Tool"
            items={mcpItems}
            value={mcpToolName}
            inputId="plugin-dev-mcp-arguments"
            inputLabel="Arguments JSON"
            input={mcpArguments}
            result={mcpResult}
            actionLabel="运行 MCP Tool"
            busy={localBusy}
            enabled={detail.connection.connected && Boolean(mcpToolName)}
            onValueChange={setMcpToolName}
            onInputChange={setMcpArguments}
            onRun={() => void runMcpTool()}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function JsonTestPanel({
  label,
  items,
  value,
  inputId,
  inputLabel,
  input,
  result,
  actionLabel,
  busy,
  enabled,
  onValueChange,
  onInputChange,
  onRun,
}: {
  label: string;
  items: Array<{ value: string; label: string }>;
  value: string | null;
  inputId: string;
  inputLabel: string;
  input: string;
  result: string | null;
  actionLabel: string;
  busy: boolean;
  enabled: boolean;
  onValueChange: (value: string | null) => void;
  onInputChange: (value: string) => void;
  onRun: () => void;
}) {
  return (
    <div className="plugin-dev-test-grid">
      <FieldGroup>
        <Field>
          <FieldLabel>{label}</FieldLabel>
          <Select items={items} value={value} onValueChange={onValueChange}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {items.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={inputId}>{inputLabel}</FieldLabel>
          <Textarea
            id={inputId}
            className="font-mono"
            rows={12}
            value={input}
            onChange={(event) => onInputChange(event.target.value)}
          />
        </Field>
        <Button onClick={onRun} disabled={busy || !enabled}>
          {busy ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <Play data-icon="inline-start" />
          )}
          {actionLabel}
        </Button>
      </FieldGroup>
      <section>
        <h2>结果</h2>
        <pre className="plugin-dev-command-result">
          {result ?? "运行后在此显示返回值或错误。"}
        </pre>
      </section>
    </div>
  );
}
