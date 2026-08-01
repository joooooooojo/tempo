import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Braces,
  FolderOpen,
  Plus,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { useOptionalMainPanelAppBarChrome } from "@/apps/appBarChrome";
import { useOptionalAppNavigation } from "@/apps/navigation";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { api } from "@/lib/api";
import { useSaveShortcut } from "@/hooks/useSaveShortcut";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { mirrorPluginDevLogToConsole } from "@/lib/pluginDevLog";
import {
  cloneManifest,
  parseEditableManifest,
  resolvedManifestKind,
  stringifyManifest,
  type EditablePluginManifest,
  type PluginKind,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import { RuntimeWorkspace } from "@/builtin-plugins/plugin-dev-assistant/pages/runtime-workspace";
import { ManifestWorkspace } from "@/builtin-plugins/plugin-dev-assistant/pages/manifest-workspace";
import { ProjectSwitcher } from "@/builtin-plugins/plugin-dev-assistant/components/ProjectSwitcher.tsx";
import {
  messageOf,
  normalizePreferences,
  type ManifestMode,
  type WorkspaceTab,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import { WorkspaceTabs } from "@/builtin-plugins/plugin-dev-assistant/components/WorkspaceTabs.tsx";
import { WorkspaceKeepAlive } from "@/builtin-plugins/plugin-dev-assistant/components/WorkspaceKeepAlive";
import type {
  PluginDevLogEvent,
  PluginDevPreferences,
  PluginDevProject,
  PluginDevProjectDetail,
} from "@/types";

const PROJECT_TEMPLATE_ITEMS = [
  { value: "ui", label: "UI · Vite" },
  { value: "hybrid", label: "Hybrid · Vite" },
  { value: "headless", label: "Headless · Vite" },
] as const;

const PROJECT_TEMPLATE_DESCRIPTIONS: Record<PluginKind, string> = {
  ui: "页面插件。构建后 dist 包含 index.html 与 manifest.json。",
  hybrid: "页面与 Runtime。构建后 dist 同时包含 index.html、main.mjs 与 manifest.json。",
  headless: "仅 Runtime。构建后 dist 包含 main.mjs 与 manifest.json。",
};

export function PluginDevAssistantPage() {
  const navigation = useOptionalAppNavigation();
  const appBarChrome = useOptionalMainPanelAppBarChrome();
  const setAppBarChrome = appBarChrome?.setChrome;
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
  const [pendingRemoval, setPendingRemoval] =
    useState<PluginDevProject | null>(null);
  const [logs, setLogs] = useState<PluginDevLogEvent[]>([]);

  const applyDetail = useCallback((next: PluginDevProjectDetail) => {
    setDetail(next);
    setActiveProjectId(next.project.id);
    setManifestRaw(next.manifest.raw);
    setPreferences(normalizePreferences(next.preferences));
    setLogs([]);
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

  const manifest = useMemo(
    () => parseEditableManifest(manifestRaw),
    [manifestRaw],
  );
  const kind = resolvedManifestKind(manifest);

  useEffect(() => {
    const unlisten = listen<PluginDevLogEvent>("plugin-dev://log", (event) => {
      if (event.payload.pluginId !== detail?.project.pluginId) return;
      if (kind === "headless") {
        setLogs((current) => [...current.slice(-199), event.payload]);
      }
      if (kind !== "headless") {
        mirrorPluginDevLogToConsole(event.payload);
      }
    });
    return () => void unlisten.then((dispose) => dispose());
  }, [detail?.project.pluginId, kind]);

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
      const entry: PluginDevLogEvent = {
        pluginId: event.payload.pluginId,
        source: "host",
        message: event.payload.message
          ? `${event.payload.state}: ${event.payload.message}`
          : event.payload.state,
        at: new Date().toISOString(),
      };
      if (kind === "headless") {
        setLogs((current) => [...current.slice(-199), entry]);
      } else {
        mirrorPluginDevLogToConsole(entry);
      }
      if (event.payload.state === "ready" || event.payload.state === "failed") {
        void loadProject(activeProjectId);
      }
    });
    return () => void unlisten.then((dispose) => dispose());
  }, [activeProjectId, detail?.project.pluginId, kind, loadProject]);
  const firstApp = manifest?.contributes.apps[0];

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
    if (!detail || busy) return;
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

  useSaveShortcut(
    () => {
      if (workspaceTab === "manifest") void saveManifest();
      else void savePreferences();
    },
    {
      active: Boolean(detail),
      enabled: !busy && (workspaceTab !== "runtime" || Boolean(preferences)),
    },
  );

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
      toast.success("已断开运行连接");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const forgetProject = async () => {
    if (!pendingRemoval) return;
    const project = pendingRemoval;
    setBusy(true);
    try {
      await api.forgetPluginDevProject(project.id);
      setPendingRemoval(null);
      if (project.id === activeProjectId) {
        setActiveProjectId(null);
        setDetail(null);
      }
      await loadProjects();
      toast.success("已从最近项目中移除");
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const openConnectedApp = () => {
    if (!detail?.connection.connected || !detail.project.pluginId || !firstApp)
      return;
    navigation?.openApp(`${detail.project.pluginId}/${firstApp.id}`);
  };

  useEffect(() => {
    if (!setAppBarChrome) return;
    setAppBarChrome({
      leading: (
        <>
          <ProjectSwitcher
            projects={projects}
            activeProjectId={activeProjectId}
            disabled={busy}
            onSelect={(projectId) => void loadProject(projectId)}
            onDelete={setPendingRemoval}
            onCreate={() => setCreateOpen(true)}
            onOpen={() => void openProject()}
          />
          {detail ? (
            <WorkspaceTabs
              value={workspaceTab}
              onChange={setWorkspaceTab}
              manifestMode={manifestMode}
              onManifestModeChange={setManifestMode}
            />
          ) : null}
        </>
      ),
    });
    return () => setAppBarChrome({});
  }, [
    activeProjectId,
    busy,
    detail,
    loadProject,
    projects,
    setAppBarChrome,
    workspaceTab,
    manifestMode,
  ]);

  return (
    <div className="plugin-dev-shell">
      <main className="plugin-dev-main">
        {loading && !detail ? (
          <div className="plugin-dev-loading">
            <Spinner />
            正在读取项目
          </div>
        ) : detail ? (
          <div className="plugin-dev-workspace">
            <div className="plugin-dev-workspace-keepalive">
              <WorkspaceKeepAlive active={workspaceTab === "manifest"}>
                <ManifestWorkspace
                  detail={detail}
                  manifest={manifest}
                  raw={manifestRaw}
                  mode={manifestMode}
                  busy={busy}
                  verifyContext={{
                    projectId: detail.project.id,
                    pluginId: detail.project.pluginId ?? null,
                    connected: detail.connection.connected,
                  }}
                  onRawChange={setManifestRaw}
                  onUpdate={updateManifest}
                  onSave={() => void saveManifest()}
                  onForget={() => setPendingRemoval(detail.project)}
                />
              </WorkspaceKeepAlive>
              {preferences ? (
                <WorkspaceKeepAlive active={workspaceTab === "runtime"}>
                  <RuntimeWorkspace
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
                </WorkspaceKeepAlive>
              ) : null}
            </div>
          </div>
        ) : (
          <Empty className="h-full rounded-none border-0">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Braces />
              </EmptyMedia>
              <EmptyTitle>选择一个插件项目</EmptyTitle>
              <EmptyDescription>
                {loadError ?? "项目根目录中的 manifest.json 是运行连接的入口。"}
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button size="lg" onClick={() => setCreateOpen(true)}>
                <Plus data-icon="inline-start" />
                创建项目
              </Button>
              <Button size="lg" variant="outline" onClick={() => void openProject()}>
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
              从模板创建完整 Vite 工程，构建后的 dist 可直接供 Tempo 使用。
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
                    placeholder="输入/选择项目根目录"
                  />
                  <Button
                    variant="outline"
                    size="icon-lg"
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
                <FieldLabel>项目模板</FieldLabel>
                <Select
                  items={PROJECT_TEMPLATE_ITEMS}
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
                      {PROJECT_TEMPLATE_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                <p className="text-muted-foreground text-sm">
                  {PROJECT_TEMPLATE_DESCRIPTIONS[createKind]}
                </p>
              </Field>
            </FieldGroup>
          </DialogContent>
          <DialogFooter>
            <Button size="lg" variant="outline" onClick={() => setCreateOpen(false)}>
              取消
            </Button>
            <Button
              size="lg"
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

      <AlertDialog
        open={pendingRemoval !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setPendingRemoval(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogMedia className="bg-destructive/10 text-destructive">
              <Trash2 />
            </AlertDialogMedia>
            <AlertDialogTitle>
              移除{pendingRemoval?.name ? `“${pendingRemoval.name}”` : "这个项目"}？
            </AlertDialogTitle>
            <AlertDialogDescription>
              只会从插件开发助手的项目列表中移除记录，不会删除本地项目文件或目录。
              {pendingRemoval?.rootPath ? (
                <span
                  className="mt-2 block break-all font-mono text-xs"
                  title={pendingRemoval.rootPath}
                >
                  {pendingRemoval.rootPath}
                </span>
              ) : null}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>取消</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={busy}
              onClick={() => void forgetProject()}
            >
              {busy ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <Trash2 data-icon="inline-start" />
              )}
              {busy ? "正在移除" : "移除记录"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
