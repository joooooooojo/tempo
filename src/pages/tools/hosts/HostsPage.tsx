import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import {
  History,
  Loader2,
  Plus,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { HostsBackup, HostsProfile, HostsWorkspace } from "@/types";

type EditorTarget = "system" | "public" | { profileId: string };

function sameTarget(a: EditorTarget, b: EditorTarget) {
  if (a === b) return true;
  return typeof a !== "string" && typeof b !== "string" && a.profileId === b.profileId;
}

export function HostsPage() {
  const [workspace, setWorkspace] = useState<HostsWorkspace | null>(null);
  const [editorTarget, setEditorTarget] = useState<EditorTarget>("public");
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [authorizing, setAuthorizing] = useState(false);
  const [backups, setBackups] = useState<HostsBackup[]>([]);
  const [createOpen, setCreateOpen] = useState(false);
  const [backupOpen, setBackupOpen] = useState(false);
  const [profileName, setProfileName] = useState("");
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const contentCache = useRef(new Map<string, string>());
  const editorTargetRef = useRef(editorTarget);
  const dirtyRef = useRef(dirty);

  editorTargetRef.current = editorTarget;
  dirtyRef.current = dirty;

  useEffect(() => {
    const el = editorRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [content, editorTarget, loading]);

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
      })
    );
  }, []);

  const applyWorkspace = useCallback((next: HostsWorkspace, keepTarget?: EditorTarget) => {
    setWorkspace(next);
    const target = keepTarget ?? "public";
    setEditorTarget(target);
    if (target === "system") {
      setContent(next.systemContent);
    } else if (target === "public") {
      setContent(next.publicContent);
    }
    setDirty(false);
  }, []);

  const refreshBackups = useCallback(async () => {
    setBackups(await api.listHostsBackups());
  }, []);

  const load = useCallback(
    async (keepTarget?: EditorTarget) => {
      setLoading(true);
      try {
        const next = await api.getHostsWorkspace();
        contentCache.current.clear();
        const target = keepTarget ?? "public";
        if (typeof target !== "string") {
          setWorkspace(next);
          try {
            const text = await api.getHostsProfileContent(target.profileId);
            contentCache.current.set(target.profileId, text);
            setEditorTarget(target);
            setContent(text);
            setDirty(false);
          } catch {
            applyWorkspace(next, "public");
          }
        } else {
          applyWorkspace(next, target);
        }
        void prefetchProfileContents(next.profiles);
        await refreshBackups();
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      } finally {
        setLoading(false);
      }
    },
    [applyWorkspace, prefetchProfileContents, refreshBackups]
  );

  useEffect(() => {
    void load();
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
  };

  const openPublic = () => {
    if (!workspace) return;
    if (sameTarget(editorTargetRef.current, "public")) return;
    if (dirtyRef.current && !confirm("当前编辑未保存，切换将丢弃修改。继续？")) return;
    switchTo("public", workspace.publicContent);
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
    if (!workspace || editorTarget === "system") return;
    setSaving(true);
    try {
      if (editorTarget === "public") {
        const next = await api.saveHostsPublic(content);
        applyWorkspace(next, "public");
        toast.success("公共配置已保存并应用到系统");
      } else {
        const profileId = editorTarget.profileId;
        const profile = workspace.profiles.find((p) => p.id === profileId);
        const name = profile?.name ?? "未命名";
        const saved = await api.saveHostsProfile(name, content, profileId);
        contentCache.current.set(saved.id, content);
        const next = await api.getHostsWorkspace();
        applyWorkspace(next, { profileId: saved.id });
        setContent(content);
        setDirty(false);
        toast.success(
          saved.active ? "自定义配置已保存并同步到系统" : "自定义配置已保存（未激活，未改系统）"
        );
      }
      await refreshBackups();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const createProfile = async () => {
    const name = profileName.trim();
    if (!name) {
      toast.error("请输入配置名称");
      return;
    }
    try {
      const saved = await api.saveHostsProfile(name, "# 自定义 hosts\n", null);
      contentCache.current.set(saved.id, "# 自定义 hosts\n");
      setCreateOpen(false);
      setProfileName("");
      const next = await api.getHostsWorkspace();
      setWorkspace(next);
      setEditorTarget({ profileId: saved.id });
      setContent("# 自定义 hosts\n");
      setDirty(false);
      toast.success("已创建自定义配置");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const activate = async (profileId: string | null) => {
    const label = profileId
      ? workspace?.profiles.find((p) => p.id === profileId)?.name ?? "该配置"
      : "仅公共配置";
    if (!confirm(`激活「${label}」并写入系统 hosts（公共 + 激活配置）？`)) return;
    setSaving(true);
    try {
      if (dirty) {
        if (editorTarget === "public") {
          await api.saveHostsPublic(content);
        } else if (typeof editorTarget !== "string") {
          const profile = workspace?.profiles.find((p) => p.id === editorTarget.profileId);
          if (profile) {
            await api.saveHostsProfile(profile.name, content, profile.id);
            contentCache.current.set(profile.id, content);
          }
        }
      }
      const next = await api.activateHostsProfile(profileId);
      const keep =
        editorTarget === "public"
          ? "public"
          : typeof editorTarget !== "string"
            ? { profileId: editorTarget.profileId }
            : "public";
      if (keep === "public") {
        applyWorkspace(next, "public");
      } else {
        setWorkspace(next);
        const text =
          contentCache.current.get(keep.profileId) ??
          (await api.getHostsProfileContent(keep.profileId));
        contentCache.current.set(keep.profileId, text);
        setEditorTarget(keep);
        setContent(text);
        setDirty(false);
      }
      await refreshBackups();
      toast.success(profileId ? `已激活「${label}」` : "已取消自定义配置，系统仅保留公共部分");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const deleteProfile = async (profile: HostsProfile) => {
    if (
      !confirm(
        `删除自定义配置「${profile.name}」？${profile.active ? "（当前已激活，删除后系统将只保留公共配置）" : ""}`
      )
    ) {
      return;
    }
    try {
      const next = await api.deleteHostsProfile(profile.id);
      contentCache.current.delete(profile.id);
      if (typeof editorTarget !== "string" && editorTarget.profileId === profile.id) {
        applyWorkspace(next, "public");
      } else {
        setWorkspace(next);
      }
      await refreshBackups();
      toast.success("已删除");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
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
    if (!confirm("恢复该备份将覆盖公共/激活配置并写回系统，继续？")) return;
    setSaving(true);
    try {
      const next = await api.restoreHostsBackup(backup.id);
      applyWorkspace(next, "public");
      await refreshBackups();
      setBackupOpen(false);
      toast.success("已从备份恢复");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  let editingLabel = "自定义配置";
  if (editorTarget === "system") {
    editingLabel = "系统 hosts";
  } else if (editorTarget === "public") {
    editingLabel = "公共配置";
  } else {
    editingLabel =
      workspace?.profiles.find((p) => p.id === editorTarget.profileId)?.name ?? "自定义配置";
  }

  const isSystem = editorTarget === "system";
  const isPublic = editorTarget === "public";
  const readOnly = isSystem;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {workspace && !workspace.authorized && (
        <div className="flex shrink-0 items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-[12px] text-amber-900 dark:text-amber-100">
          <p className="min-w-0 flex-1">
            首次写入需要管理员权限。点击「一键授权」后将授予当前用户对 hosts 的修改权限，之后可直接保存。
          </p>
          <Button size="sm" onClick={() => void authorize()} disabled={authorizing}>
            {authorizing ? <Loader2 className="size-3.5 animate-spin" /> : <ShieldCheck className="size-3.5" />}
            一键授权
          </Button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-60 shrink-0 flex-col border-r border-border/60">
          <ScrollArea className="min-h-0 flex-1" viewportClassName="px-2 py-2">
            <button
              type="button"
              className={cn(
                "mb-1 w-full rounded-lg px-2.5 py-2 text-left text-[12px]",
                isSystem ? "bg-foreground/8" : "hover:bg-foreground/5"
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

            <button
              type="button"
              className={cn(
                "mb-1 flex w-full items-center gap-1.5 rounded-lg px-2.5 py-2 text-left text-[12px]",
                isPublic ? "bg-foreground/8" : "hover:bg-foreground/5"
              )}
              onClick={openPublic}
            >
              <span
                className="hosts-active-pulse"
                title="始终生效"
                aria-label="公共配置始终生效"
              />
              <div className="min-w-0 flex-1">
                <div className="font-medium">公共 hosts</div>
                <div className="mt-0.5 text-[10px] text-muted-foreground">始终写入系统</div>
              </div>
            </button>

            <div className="my-1.5 border-t border-border/60" />

            {(workspace?.profiles ?? []).map((profile) => {
              const selected =
                typeof editorTarget !== "string" && editorTarget.profileId === profile.id;
              return (
                <div
                  key={profile.id}
                  role="button"
                  tabIndex={0}
                  className={cn(
                    "group mb-1 flex w-full items-center gap-1.5 rounded-lg px-2.5 py-2 text-[12px]",
                    "cursor-pointer outline-none transition-colors",
                    selected ? "bg-foreground/8" : "hover:bg-foreground/5"
                  )}
                  onClick={() => openProfile(profile)}
                  onDoubleClick={(e) => {
                    e.preventDefault();
                    void activate(profile.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") openProfile(profile);
                  }}
                  title="单击选中编辑，双击激活并写入系统"
                >
                  {profile.active ? (
                    <span
                      className="hosts-active-pulse shrink-0"
                      title="已激活"
                      aria-label="已激活"
                    />
                  ) : (
                    <span className="size-2 shrink-0" aria-hidden />
                  )}
                  <div className="min-w-0 flex-1 truncate text-left">
                    <span className="font-medium">{profile.name}</span>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-6 shrink-0 opacity-0 group-hover:opacity-100"
                    onClick={(e) => {
                      e.stopPropagation();
                      void deleteProfile(profile);
                    }}
                    title="删除"
                  >
                    <Trash2 className="size-3" />
                  </Button>
                </div>
              );
            })}

            <button
              type="button"
              className={cn(
                "mt-0.5 flex w-full items-center justify-center gap-1.5 rounded-lg px-2.5 py-2 text-[12px]",
                "border border-dashed border-border/80 text-muted-foreground",
                "hover:border-foreground/30 hover:bg-foreground/5 hover:text-foreground"
              )}
              onClick={() => {
                setProfileName("");
                setCreateOpen(true);
              }}
            >
              <Plus className="size-3.5" />
              添加自定义配置
            </button>
          </ScrollArea>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col p-3">
          <div className="mb-2 flex items-center justify-between gap-2">
            <div className="text-[13px] font-medium">
              {readOnly ? "正在查看：" : "正在编辑："}
              {editingLabel}
            </div>
            {workspace?.managed === false && !isSystem && (
              <span className="rounded-md bg-muted px-2 py-1 text-[10px] text-muted-foreground">
                系统尚未写入分区标记；保存后将建立「公共 / 自定义」分区，便于下次解析
              </span>
            )}
          </div>
          <ScrollArea
            className={cn(
              "min-h-0 flex-1 rounded-lg border border-border/60",
              readOnly ? "bg-muted/40" : "bg-background/50"
            )}
            viewportClassName="p-0"
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
                readOnly && "cursor-default text-muted-foreground"
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
          <Button variant="outline" onClick={() => void load(editorTarget)} disabled={loading}>
            <RefreshCw className={cn(loading && "animate-spin")} />
            刷新
          </Button>
          <Button onClick={() => void saveCurrent()} disabled={saving || !dirty || readOnly}>
            {saving ? <Loader2 className="animate-spin" /> : <Save />}
            保存
          </Button>
        </div>
      </footer>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogPanel className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>新建自定义配置</DialogTitle>
          </DialogHeader>
          <DialogContent>
            <Input
              value={profileName}
              onChange={(e) => setProfileName(e.target.value)}
              placeholder="例如：公司环境 / 测试环境"
              autoFocus
            />
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              取消
            </Button>
            <Button onClick={() => void createProfile()}>创建</Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog open={backupOpen} onOpenChange={setBackupOpen}>
        <DialogPanel className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>备份记录</DialogTitle>
          </DialogHeader>
          <DialogContent className="max-h-[min(420px,55vh)] space-y-1.5 overflow-y-auto px-4 py-3">
            {backups.length === 0 ? (
              <p className="py-6 text-center text-[12px] text-muted-foreground">暂无备份</p>
            ) : (
              backups.map((backup) => (
                <button
                  key={backup.id}
                  type="button"
                  className="w-full rounded-lg border border-border/50 px-3 py-2.5 text-left transition-colors hover:bg-foreground/5"
                  onClick={() => void restoreBackup(backup)}
                  title={backup.preview}
                >
                  <div className="text-[13px] font-medium">{backup.createdAt}</div>
                  <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
                    {backup.source}
                    {backup.preview ? ` · ${backup.preview}` : ""}
                  </div>
                </button>
              ))
            )}
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBackupOpen(false)}>
              关闭
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </div>
  );
}
