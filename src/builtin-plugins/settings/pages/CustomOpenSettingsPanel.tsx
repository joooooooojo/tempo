import { useCallback, useEffect, useState } from "react";
import { FilePlus2, FolderPlus, Loader2, Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import type { CustomLauncherEntry } from "@/types";
import { Section } from "@/builtin-plugins/settings/pages/shared";

function isWindowsHost() {
  return /Windows/i.test(navigator.userAgent);
}

function kindLabel(kind: string) {
  switch (kind) {
    case "folder":
      return "文件夹";
    case "shortcut":
      return "快捷方式";
    default:
      return "文件";
  }
}

export function CustomOpenSettingsPanel() {
  const [entries, setEntries] = useState<CustomLauncherEntry[]>([]);
  const [loading, setLoading] = useState(true);
  /** Blocks add-actions only; row actions use per-row busyId to avoid list-wide opacity flash. */
  const [busy, setBusy] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const load = useCallback(async () => {
    const next = await api.listCustomLauncherEntries();
    setEntries(next);
  }, []);

  useEffect(() => {
    setLoading(true);
    load()
      .catch((error) => {
        toast.error(error instanceof Error ? error.message : String(error));
      })
      .finally(() => setLoading(false));
  }, [load]);

  const refreshLauncher = async () => {
    try {
      await api.refreshLauncherApps();
    } catch {
      // Index refresh is best-effort; entries are already persisted.
    }
  };

  const addPaths = async (paths: string[]) => {
    if (!paths.length || busy) return;
    setBusy(true);
    try {
      await api.addCustomLauncherEntries(paths);
      await load();
      await refreshLauncher();
      toast.success(`已添加 ${paths.length} 项`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const addFiles = async () => {
    const selected = await openNativeFileDialog({
      multiple: true,
      title: "添加文件或快捷方式",
      filters: isWindowsHost()
        ? [
            { name: "常用文件", extensions: ["*"] },
            { name: "快捷方式", extensions: ["lnk", "url"] },
          ]
        : undefined,
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await addPaths(paths);
  };

  const addFolders = async () => {
    const selected = await openNativeFileDialog({
      directory: true,
      multiple: true,
      title: "添加文件夹",
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await addPaths(paths);
  };

  const removeEntry = async (entry: CustomLauncherEntry) => {
    if (busyId || busy) return;
    setBusyId(entry.id);
    setEntries((current) => current.filter((item) => item.id !== entry.id));
    try {
      await api.removeCustomLauncherEntry(entry.id);
      await refreshLauncher();
      toast.success("已移除");
    } catch (error) {
      setEntries((current) => [...current, entry]);
      toast.error(error instanceof Error ? error.message : String(error));
      await load();
    } finally {
      setBusyId(null);
    }
  };

  const startRename = (entry: CustomLauncherEntry) => {
    setRenamingId(entry.id);
    setRenameValue(entry.name);
  };

  const commitRename = async (entry: CustomLauncherEntry) => {
    const nextName = renameValue.trim();
    setRenamingId(null);
    if (!nextName || nextName === entry.name) return;
    if (busyId || busy) return;
    setBusyId(entry.id);
    setEntries((current) =>
      current.map((item) => (item.id === entry.id ? { ...item, name: nextName } : item))
    );
    try {
      await api.renameCustomLauncherEntry(entry.id, nextName);
      await refreshLauncher();
      toast.success("已重命名");
    } catch (error) {
      setEntries((current) =>
        current.map((item) => (item.id === entry.id ? { ...item, name: entry.name } : item))
      );
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="settings-panel-stack">
      <Section title=''>
        <Card>
          <CardContent className="space-y-4 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <p className="text-[14px] font-medium">添加到索引</p>
                <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
                  选择本地文件、文件夹后，可在主面板搜索打开。
                </p>
              </div>
              <div className="flex shrink-0 flex-wrap gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void addFiles()}
                >
                  <FilePlus2 className="h-3.5 w-3.5" />
                  添加文件
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void addFolders()}
                >
                  <FolderPlus className="h-3.5 w-3.5" />
                  添加文件夹
                </Button>
              </div>
            </div>

            {loading ? (
              <div className="flex items-center gap-2 py-6 text-[13px] text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                加载中…
              </div>
            ) : entries.length === 0 ? (
              <p className="rounded-lg border border-dashed border-border/70 px-3 py-6 text-center text-[12px] text-muted-foreground">
                尚未添加自定义打开项
              </p>
            ) : (
              <ul className="divide-y divide-border/60 rounded-lg border border-border/60">
                {entries.map((entry) => {
                  const renaming = renamingId === entry.id;
                  const rowBusy = busyId === entry.id;
                  return (
                    <li
                      key={entry.id}
                      className="flex items-center gap-3 px-3 py-2.5"
                    >
                      <div
                        className={cn(
                          "flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border/50 bg-foreground/[0.03]",
                        )}
                      >
                        {entry.iconDataUrl ? (
                          <img
                            src={entry.iconDataUrl}
                            alt=""
                            className="h-7 w-7 object-contain"
                          />
                        ) : (
                          <span className="text-[11px] font-medium text-muted-foreground">
                            {kindLabel(entry.kind).slice(0, 1)}
                          </span>
                        )}
                      </div>
                      <div className="min-w-0 flex-1">
                        {renaming ? (
                          <Input
                            autoFocus
                            value={renameValue}
                            className="h-8 text-[13px]"
                            onChange={(event) => setRenameValue(event.target.value)}
                            onBlur={() => void commitRename(entry)}
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                void commitRename(entry);
                              }
                              if (event.key === "Escape") {
                                setRenamingId(null);
                              }
                            }}
                          />
                        ) : (
                          <p className="truncate text-[13px] font-medium">{entry.name}</p>
                        )}
                        <p className="mt-0.5 truncate text-[11px] text-muted-foreground" title={entry.path}>
                          {kindLabel(entry.kind)} · {entry.path}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          disabled={rowBusy || renaming}
                          aria-label="重命名"
                          onClick={() => startRename(entry)}
                        >
                          <Pencil className="h-3.5 w-3.5" />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          disabled={rowBusy}
                          aria-label="删除"
                          onClick={() => void removeEntry(entry)}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </CardContent>
        </Card>
      </Section>
    </div>
  );
}
