import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Cloud,
  FileText,
  FolderOpen,
  History,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
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
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import { ScrollArea } from "@/components/ui/scroll-area";
import { SAVE_SHORTCUT_LABEL, useSaveShortcut } from "@/hooks/useSaveShortcut";
import type { HostsBackup, HostsProfile, HostsWorkspace } from "@/types";
import {
  EMPTY_CREATE_DRAFT,
  HostsDialogs,
  type CreateHostsDraft,
} from "@/builtin-plugins/hosts/pages/HostsDialogs";

type EditorTarget = "system" | { profileId: string };

function sameTarget(a: EditorTarget, b: EditorTarget) {
  if (a === "system" || b === "system") return a === b;
  return a.profileId === b.profileId;
}

function formatFetchedAt(value?: string | null) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function HostsPage() {
  const [workspace, setWorkspace] = useState<HostsWorkspace | null>(null);
  const [editorTarget, setEditorTarget] = useState<EditorTarget>("system");
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [authorizing, setAuthorizing] = useState(false);
  const [backups, setBackups] = useState<HostsBackup[]>([]);
  const [formOpen, setFormOpen] = useState(false);
  const [formMode, setFormMode] = useState<"create" | "edit">("create");
  const [formDraft, setFormDraft] = useState<CreateHostsDraft>(EMPTY_CREATE_DRAFT);
  const [backupOpen, setBackupOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<HostsProfile | null>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const editorViewportRef = useRef<HTMLDivElement>(null);
  const contentCache = useRef(new Map<string, string>());
  const editorTargetRef = useRef(editorTarget);
  const dirtyRef = useRef(dirty);
  /** Skip hosts-changed → load while this page is applying its own mutation. */
  const localMutatingRef = useRef(false);

  editorTargetRef.current = editorTarget;
  dirtyRef.current = dirty;

  useEffect(() => {
    const el = editorRef.current;
    if (!el) return;
    const viewport = editorViewportRef.current;
    const scrollTop = viewport?.scrollTop ?? 0;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
    if (viewport) viewport.scrollTop = scrollTop;
  }, [content, editorTarget, loading]);

  const selectedProfile: HostsProfile | null =
    typeof editorTarget !== "string"
      ? (workspace?.profiles.find((p) => p.id === editorTarget.profileId) ?? null)
      : null;
  const isRemote = selectedProfile?.kind === "remote";
  const isSystem = editorTarget === "system";
  const readOnly = isSystem || isRemote;

  const prefetchProfileContents = useCallback(async (profiles: HostsProfile[]) => {
    await Promise.all(
      profiles.map(async (profile) => {
        if (contentCache.current.has(profile.id)) return;
        try {
          const text = await api.getHostsProfileContent(profile.id);
          contentCache.current.set(profile.id, text);
        } catch {
          /* ignore prefetch errors */
        }
      }),
    );
  }, []);

  const applyWorkspace = useCallback((next: HostsWorkspace, keepTarget?: EditorTarget) => {
    setWorkspace(next);
    const target = keepTarget ?? "system";
    setEditorTarget(target);
    if (target === "system") {
      setContent(next.systemContent);
      setDirty(false);
      return;
    }
    const cached = contentCache.current.get(target.profileId);
    if (cached !== undefined) {
      setContent(cached);
      setDirty(false);
    }
  }, []);

  const refreshBackups = useCallback(async () => {
    setBackups(await api.listHostsBackups());
  }, []);

  const load = useCallback(
    async (keepTarget?: EditorTarget, options?: { soft?: boolean }) => {
      const soft = options?.soft ?? false;
      if (!soft) setLoading(true);
      try {
        const next = await api.getHostsWorkspace();
        if (!soft) contentCache.current.clear();
        const target = keepTarget ?? editorTargetRef.current;
        if (typeof target !== "string") {
          setWorkspace(next);
          try {
            const cached = soft ? contentCache.current.get(target.profileId) : undefined;
            const text =
              cached !== undefined
                ? cached
                : await api.getHostsProfileContent(target.profileId);
            contentCache.current.set(target.profileId, text);
            setEditorTarget(target);
            if (!dirtyRef.current) {
              setContent(text);
              setDirty(false);
            }
          } catch {
            applyWorkspace(next, "system");
          }
        } else {
          if (soft && !dirtyRef.current) {
            setWorkspace(next);
            setContent(next.systemContent);
            setDirty(false);
          } else if (!soft) {
            applyWorkspace(next, "system");
          } else {
            setWorkspace(next);
          }
        }
        void prefetchProfileContents(next.profiles);
        await refreshBackups();
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      } finally {
        if (!soft) setLoading(false);
      }
    },
    [applyWorkspace, prefetchProfileContents, refreshBackups],
  );

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen("hosts-changed", () => {
      if (cancelled || dirtyRef.current || localMutatingRef.current) return;
      void load(editorTargetRef.current, { soft: true });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  const switchTo = (target: EditorTarget, nextContent: string) => {
    startTransition(() => {
      setEditorTarget(target);
      setContent(nextContent);
      setDirty(false);
    });
  };

  const openSystem = () => {
    if (!workspace) return;
    if (sameTarget(editorTargetRef.current, "system")) return;
    if (dirtyRef.current && !confirm("当前编辑未保存，切换将丢弃修改。继续？")) return;
    switchTo("system", workspace.systemContent);
    void (async () => {
      try {
        const next = await api.getHostsWorkspace();
        setWorkspace(next);
        if (!sameTarget(editorTargetRef.current, "system") || dirtyRef.current) return;
        setContent(next.systemContent);
        setDirty(false);
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    })();
  };

  const openProfile = (profile: HostsProfile) => {
    const target: EditorTarget = { profileId: profile.id };
    if (sameTarget(editorTargetRef.current, target)) return;
    if (dirtyRef.current && !confirm("当前编辑未保存，切换将丢弃修改。继续？")) return;

    const cached = contentCache.current.get(profile.id);
    if (cached !== undefined) {
      switchTo(target, cached);
      return;
    }

    void (async () => {
      try {
        const text = await api.getHostsProfileContent(profile.id);
        contentCache.current.set(profile.id, text);
        if (dirtyRef.current) return;
        switchTo(target, text);
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    })();
  };

  const authorize = async () => {
    setAuthorizing(true);
    try {
      const next = await api.authorizeHostsWrite();
      setWorkspace(next);
      toast.success("授权成功，之后保存无需再提权");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setAuthorizing(false);
    }
  };

  const saveCurrent = async () => {
    if (!workspace || editorTarget === "system" || !selectedProfile) return;
    if (saving || !dirty || selectedProfile.kind !== "local") return;
    setSaving(true);
    try {
      const saved = await api.saveHostsProfile({
        id: selectedProfile.id,
        name: selectedProfile.name,
        kind: "local",
        content,
      });
      contentCache.current.set(saved.id, content);
      const next = await api.getHostsWorkspace();
      setWorkspace(next);
      setDirty(false);
      toast.success(saved.active ? "已保存并同步到系统" : "已保存（未激活，未改系统）");
      await refreshBackups();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  useSaveShortcut(() => void saveCurrent(), {
    enabled: Boolean(workspace) && !saving && dirty && !readOnly,
  });

  const syncRemote = async () => {
    if (!selectedProfile || selectedProfile.kind !== "remote") return;
    setSyncing(true);
    try {
      const next = await api.refreshHostsRemoteProfile(selectedProfile.id);
      const text = await api.getHostsProfileContent(selectedProfile.id);
      contentCache.current.set(selectedProfile.id, text);
      setWorkspace(next);
      setContent(text);
      setDirty(false);
      toast.success("远程 hosts 已同步");
      await refreshBackups();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      void load(editorTargetRef.current);
    } finally {
      setSyncing(false);
    }
  };

  const openCreateForm = () => {
    setFormMode("create");
    setFormDraft(EMPTY_CREATE_DRAFT);
    setFormOpen(true);
  };

  const openEditForm = () => {
    if (!selectedProfile) return;
    setFormMode("edit");
    setFormDraft({
      kind: selectedProfile.kind === "remote" ? "remote" : "local",
      name: selectedProfile.name,
      localMode: "blank",
      importPath: "",
      remoteUrl: selectedProfile.remoteUrl ?? "",
      refreshIntervalSecs: selectedProfile.refreshIntervalSecs ?? 0,
    });
    setFormOpen(true);
  };

  const submitForm = async () => {
    const name = formDraft.name.trim();
    if (!name) {
      toast.error("请输入标题");
      return;
    }
    setSaving(true);
    try {
      if (formMode === "edit") {
        if (!selectedProfile) return;
        await api.saveHostsProfile({
          id: selectedProfile.id,
          name,
          kind: selectedProfile.kind,
          remoteUrl:
            selectedProfile.kind === "remote" ? formDraft.remoteUrl.trim() : undefined,
          refreshIntervalSecs:
            selectedProfile.kind === "remote" ? formDraft.refreshIntervalSecs : undefined,
        });
        setFormOpen(false);
        const next = await api.getHostsWorkspace();
        setWorkspace(next);
        toast.success("已更新配置");
        return;
      }

      const saved =
        formDraft.kind === "remote"
          ? await api.saveHostsProfile({
              name,
              kind: "remote",
              remoteUrl: formDraft.remoteUrl.trim(),
              refreshIntervalSecs: formDraft.refreshIntervalSecs,
              content: "",
            })
          : await api.saveHostsProfile({
              name,
              kind: "local",
              content:
                formDraft.localMode === "blank" ? "# 自定义 hosts\n" : undefined,
              importPath:
                formDraft.localMode === "file" ? formDraft.importPath : undefined,
            });

      if (formDraft.kind === "remote") {
        try {
          await api.refreshHostsRemoteProfile(saved.id);
        } catch (error) {
          toast.error(
            error instanceof Error
              ? `已创建，但首次同步失败：${error.message}`
              : "已创建，但首次同步失败",
          );
        }
      }

      setFormOpen(false);
      setFormDraft(EMPTY_CREATE_DRAFT);
      const text = await api.getHostsProfileContent(saved.id);
      contentCache.current.set(saved.id, text);
      const next = await api.getHostsWorkspace();
      setWorkspace(next);
      switchTo({ profileId: saved.id }, text);
      toast.success("已添加配置");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const toggleActive = async (profile: HostsProfile, active: boolean) => {
    if (localMutatingRef.current) return;
    const label = active ? "激活" : "取消激活";
    if (
      !confirm(
        `${label}「${profile.name}」？系统 hosts 将写入「原有内容 + 所有已激活配置」。`,
      )
    ) {
      return;
    }

    // Optimistic: only this row's Switch/checked icon updates; avoid disabling the
    // whole list (data-disabled:opacity-50 was flashing every thumb).
    setWorkspace((prev) => {
      if (!prev) return prev;
      const activeProfileIds = active
        ? prev.activeProfileIds.includes(profile.id)
          ? prev.activeProfileIds
          : [...prev.activeProfileIds, profile.id]
        : prev.activeProfileIds.filter((id) => id !== profile.id);
      return {
        ...prev,
        activeProfileIds,
        profiles: prev.profiles.map((p) =>
          p.id === profile.id ? { ...p, active } : p,
        ),
      };
    });

    localMutatingRef.current = true;
    setSaving(true);
    try {
      if (dirty && typeof editorTarget !== "string" && editorTarget.profileId === profile.id) {
        if (profile.kind === "local") {
          await api.saveHostsProfile({
            id: profile.id,
            name: profile.name,
            kind: "local",
            content,
          });
          contentCache.current.set(profile.id, content);
        }
      }
      const next = await api.setHostsProfileActive(profile.id, active);
      setWorkspace(next);
      if (editorTarget === "system") {
        setContent(next.systemContent);
      }
      await refreshBackups();
      toast.success(active ? `已激活「${profile.name}」` : `已取消激活「${profile.name}」`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      void load(editorTargetRef.current);
    } finally {
      setSaving(false);
      localMutatingRef.current = false;
    }
  };

  const deleteProfile = async (profile: HostsProfile) => {
    try {
      setSaving(true);
      const next = await api.deleteHostsProfile(profile.id);
      contentCache.current.delete(profile.id);
      if (typeof editorTarget !== "string" && editorTarget.profileId === profile.id) {
        applyWorkspace(next, "system");
      } else {
        setWorkspace(next);
      }
      await refreshBackups();
      setPendingDelete(null);
      toast.success("已删除");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const openBackupDialog = async () => {
    try {
      await refreshBackups();
      setBackupOpen(true);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const restoreBackup = async (backup: HostsBackup) => {
    if (!confirm("恢复该备份将覆盖当前激活集合并写回系统，继续？")) return;
    setSaving(true);
    try {
      const next = await api.restoreHostsBackup(backup.id);
      contentCache.current.clear();
      applyWorkspace(next, "system");
      await refreshBackups();
      setBackupOpen(false);
      toast.success("已从备份恢复");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const pickImportFile = async () => {
    try {
      const selected = await openNativeFileDialog({
        multiple: false,
        title: "选择 hosts 文件",
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      setFormDraft((prev) => ({ ...prev, importPath: path, localMode: "file" }));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  let editingLabel = "配置";
  if (isSystem) editingLabel = "系统 hosts";
  else if (selectedProfile) editingLabel = selectedProfile.name;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {workspace && !workspace.authorized && (
        <div className="flex shrink-0 items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-[12px] text-amber-900 dark:text-amber-100">
          <p className="min-w-0 flex-1">
            首次写入需要管理员权限。点击「一键授权」后将授予当前用户对 hosts 的修改权限，之后可直接保存。
          </p>
          <Button size="sm" onClick={() => void authorize()} disabled={authorizing}>
            {authorizing ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="size-3.5" />
            )}
            一键授权
          </Button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-60 shrink-0 flex-col border-r border-border/60">
          <div className="shrink-0 px-2 pt-2">
            <button
              type="button"
              className={cn(
                "w-full rounded-lg px-2.5 py-2 text-left text-[12px]",
                isSystem ? "bg-foreground/8" : "hover:bg-foreground/5",
              )}
              onClick={openSystem}
              title={workspace?.path}
            >
              <div className="font-medium">系统 hosts</div>
              <div className="mt-0.5 truncate text-[10px] text-muted-foreground">
                {workspace?.path || "只读查看当前文件"}
              </div>
            </button>
            <div className="my-1.5 border-t border-border/60" />
          </div>

          <ScrollArea className="min-h-0 flex-1" viewportClassName="px-2 pb-2">
            {(workspace?.profiles ?? []).map((profile) => {
              const selected =
                typeof editorTarget !== "string" && editorTarget.profileId === profile.id;
              return (
                <div
                  key={profile.id}
                  role="button"
                  tabIndex={0}
                  className={cn(
                    "group mb-0.5 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-[12px]",
                    "cursor-pointer outline-none transition-colors",
                    selected ? "bg-foreground/8" : "hover:bg-foreground/5",
                  )}
                  onClick={() => openProfile(profile)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") openProfile(profile);
                  }}
                >
                  <span className="inline-flex size-3.5 shrink-0 items-center justify-center">
                    {profile.active ? (
                      <span
                        className="hosts-active-pulse"
                        title="已激活"
                        aria-label="已激活"
                      />
                    ) : profile.kind === "remote" ? (
                      <Cloud className="size-3.5 text-muted-foreground/70" />
                    ) : (
                      <FileText className="size-3.5 text-muted-foreground/70" />
                    )}
                  </span>
                  <div className="min-w-0 flex-1 truncate text-left">
                    <div className="truncate font-medium leading-5">{profile.name}</div>
                    <div className="truncate text-[10px] leading-4 text-muted-foreground">
                      {profile.kind === "remote" ? "远程" : "本地"}
                    </div>
                  </div>
                  <div
                    className="shrink-0"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <Switch
                      size="sm"
                      checked={profile.active}
                      aria-label={profile.active ? "取消激活" : "激活"}
                      title={profile.active ? "取消激活" : "激活"}
                      className={cn(
                        "h-4 w-7 rounded-full bg-foreground/8",
                        "data-checked:bg-emerald-500/85",
                        "[&_[data-slot=switch-thumb]]:size-3 [&_[data-slot=switch-thumb]]:rounded-full",
                        "[&_[data-slot=switch-thumb]]:shadow-none [&_[data-slot=switch-thumb]]:bg-white",
                        // Override size=sm thumb travel (18px) for this compact track.
                        "[&_[data-slot=switch-thumb]]:translate-x-[2px]",
                        "[&_[data-slot=switch-thumb]]:data-checked:!translate-x-[14px]",
                      )}
                      onCheckedChange={(checked) => {
                        if (localMutatingRef.current) return;
                        void toggleActive(profile, checked);
                      }}
                    />
                  </div>
                </div>
              );
            })}

            <button
              type="button"
              className={cn(
                "mt-0.5 flex w-full items-center justify-center gap-1.5 rounded-lg px-2.5 py-2 text-[12px]",
                "border border-dashed border-border/80 text-muted-foreground",
                "hover:border-foreground/30 hover:bg-foreground/5 hover:text-foreground",
              )}
              onClick={openCreateForm}
            >
              <Plus className="size-3.5" />
              添加配置
            </button>
          </ScrollArea>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col p-3">
          <div className="mb-2 flex items-center justify-between gap-2">
            <div className="text-[13px] font-medium">
              {readOnly ? "正在查看：" : "正在编辑："}
              {editingLabel}
            </div>
            {selectedProfile?.kind === "remote" && selectedProfile.lastFetchError ? (
              <span className="truncate text-[11px] text-destructive">
                {selectedProfile.lastFetchError}
              </span>
            ) : selectedProfile?.kind === "remote" && selectedProfile.lastFetchedAt ? (
              <span className="truncate text-[11px] text-muted-foreground">
                上次同步 {formatFetchedAt(selectedProfile.lastFetchedAt)}
              </span>
            ) : null}
          </div>

          <ScrollArea
            key={editorTarget === "system" ? "system" : editorTarget.profileId}
            className={cn(
              "min-h-0 flex-1 rounded-lg border border-border/60",
              readOnly ? "bg-muted/40" : "bg-background/50",
            )}
            viewportClassName="p-0"
            viewportRef={editorViewportRef}
          >
            <textarea
              ref={editorRef}
              value={content}
              readOnly={readOnly}
              rows={1}
              onChange={(e) => {
                if (readOnly) return;
                setContent(e.target.value);
                setDirty(true);
              }}
              spellCheck={false}
              className={cn(
                "block w-full resize-none overflow-hidden border-0 bg-transparent px-3 pt-3 pb-8",
                "font-mono text-[12px] leading-5 text-foreground outline-none min-h-full!",
                readOnly && "cursor-default text-muted-foreground",
              )}
              placeholder="# hosts 内容"
            />
          </ScrollArea>
        </div>
      </div>

      <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border/60 px-4 py-3">
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={() => void openBackupDialog()}>
            <History />
            备份
          </Button>
        </div>
        <div className="flex items-center gap-2">
          {selectedProfile ? (
            <Button
              variant="outline"
              className="text-destructive hover:bg-destructive/10 hover:text-destructive"
              disabled={saving}
              onClick={() => setPendingDelete(selectedProfile)}
            >
              <Trash2 />
              删除
            </Button>
          ) : null}
          {isSystem ? (
            <Button
              variant="outline"
              disabled={!workspace?.path}
              onClick={() => {
                void api.openHostsFileLocation().catch((error) =>
                  toast.error(error instanceof Error ? error.message : String(error)),
                );
              }}
            >
              <FolderOpen />
              打开文件位置
            </Button>
          ) : selectedProfile ? (
            <>
              <Button variant="outline" disabled={saving} onClick={openEditForm}>
                <Pencil />
                编辑
              </Button>
              {isRemote ? (
                <Button disabled={syncing || saving} onClick={() => void syncRemote()}>
                  {syncing ? <Loader2 className="animate-spin" /> : <RefreshCw />}
                  立即同步
                </Button>
              ) : (
                <Button
                  onClick={() => void saveCurrent()}
                  disabled={saving || !dirty || readOnly}
                  title={`保存（${SAVE_SHORTCUT_LABEL}）`}
                >
                  {saving ? <Loader2 className="animate-spin" /> : <Save />}
                  保存
                </Button>
              )}
            </>
          ) : null}
        </div>
      </footer>

      <HostsDialogs
        formOpen={formOpen}
        onFormOpenChange={setFormOpen}
        formMode={formMode}
        draft={formDraft}
        onDraftChange={setFormDraft}
        onPickImportFile={() => void pickImportFile()}
        onSubmit={() => void submitForm()}
        saving={saving}
        backupOpen={backupOpen}
        onBackupOpenChange={setBackupOpen}
        backups={backups}
        onRestore={(backup) => void restoreBackup(backup)}
      />

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !saving) setPendingDelete(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogMedia className="bg-destructive/10 text-destructive">
              <Trash2 />
            </AlertDialogMedia>
            <AlertDialogTitle>
              删除配置「{pendingDelete?.name ?? ""}」？
            </AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.active
                ? "该配置当前已激活，删除后会从系统 hosts 中移除对应内容。"
                : "删除后无法恢复该配置内容。"}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={saving}>取消</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={saving || !pendingDelete}
              onClick={() => {
                if (pendingDelete) void deleteProfile(pendingDelete);
              }}
            >
              {saving ? <Loader2 data-icon="inline-start" className="animate-spin" /> : <Trash2 data-icon="inline-start" />}
              {saving ? "正在删除" : "确认删除"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
