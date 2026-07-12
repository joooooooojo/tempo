import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ClipboardList, Pin, Search, Trash2 } from "lucide-react";
import { toast } from "sonner";
import {
  clipboardKindLabel,
  clipboardSourceLabel,
  shelfImageSize,
} from "@/components/clipboard/ShelfCard";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import { cn, formatRelativeTime, previewLines } from "@/lib/utils";
import type { ClipboardEntry, Settings } from "@/types";

export function ClipboardPage() {
  const [entries, setEntries] = useState<ClipboardEntry[]>([]);
  const [query, setQuery] = useState("");
  const [settings, setSettings] = useState<Settings | null>(null);

  const load = useCallback(async () => {
    const [nextEntries, nextSettings] = await Promise.all([
      api.getClipboardHistory(query || undefined),
      api.getSettings(),
    ]);
    setEntries(nextEntries);
    setSettings(nextSettings);
  }, [query]);

  useEffect(() => {
    void load();
    const unlisten = listen("clipboard-update", () => void load());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 200);
    return () => window.clearTimeout(timer);
  }, [load, query]);

  const copyEntry = async (entry: ClipboardEntry) => {
    try {
      await api.copyClipboardEntry(entry.id);
      toast.success("已复制");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    }
  };

  const toggleMonitor = async (enabled: boolean) => {
    if (!settings) return;
    setSettings({ ...settings, clipboard_monitor_enabled: enabled });
    try {
      await api.updateSettings({ clipboard_monitor_enabled: enabled });
      toast.success("已保存");
    } catch (error) {
      setSettings(settings);
      toast.error(error instanceof Error ? error.message : "保存失败");
    }
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 text-xl font-bold tracking-tight">
            <ClipboardList className="h-5 w-5 text-primary" />
            剪贴板
          </h1>
          <p className="mt-1 text-[13px] text-muted-foreground">
            自动记录文字与截图，按 <kbd className="rounded bg-foreground/8 px-1.5 py-0.5 text-[11px]">F4</kbd> 快速呼出
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[12px] text-muted-foreground">记录剪贴板</span>
          <Switch
            checked={settings?.clipboard_monitor_enabled ?? true}
            onCheckedChange={(value) => void toggleMonitor(value)}
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索历史..."
            className="h-9 border-0 pl-9 glass-subtle"
          />
        </div>
        <Button
          variant="outline"
          className="h-9 border-0 glass-subtle"
          onClick={() => void api.showClipboardPicker()}
        >
          打开浮层 (F4)
        </Button>
        <Button
          variant="outline"
          className="h-9 border-0 glass-subtle"
          onClick={async () => {
            await api.clearClipboardHistory();
            toast.success("已清空未固定记录");
            void load();
          }}
        >
          清空历史
        </Button>
      </div>

      <div className="space-y-2">
        {entries.length === 0 ? (
          <Card className="border-dashed">
            <CardContent className="py-10 text-center text-[13px] text-muted-foreground">
              还没有剪贴记录，复制文字或截图试试吧
            </CardContent>
          </Card>
        ) : (
          entries.map((entry) => (
            <Card
              key={entry.id}
              className={cn(
                "cursor-pointer overflow-hidden transition-colors hover:bg-foreground/[0.03]",
                entry.pinned && "ring-1 ring-primary/30"
              )}
              onClick={() => void copyEntry(entry)}
            >
              <CardContent className="p-0">
                <div
                  className={cn(
                    "flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2 text-[11px]",
                    entry.kind === "image" ? "bg-amber-500/10" : "bg-emerald-500/10"
                  )}
                >
                  <div className="flex min-w-0 items-center gap-2">
                    <span
                      className={cn(
                        "shrink-0 font-semibold",
                        entry.kind === "image"
                          ? "text-amber-700 dark:text-amber-300"
                          : "text-emerald-600 dark:text-emerald-300"
                      )}
                    >
                      {clipboardKindLabel(entry.kind)}
                    </span>
                    <span className="text-muted-foreground">{formatRelativeTime(entry.created_at)}</span>
                    {entry.pinned && <Pin className="h-3 w-3 text-primary" />}
                  </div>
                  <span
                    className="max-w-[45%] truncate rounded bg-background/60 px-1.5 py-0.5 font-medium"
                    title={clipboardSourceLabel(entry)}
                  >
                    {clipboardSourceLabel(entry)}
                  </span>
                </div>
                <div className="flex items-start justify-between gap-3 px-3 py-3">
                  {entry.kind === "image" ? (
                    <div className="min-w-0 flex-1">
                      <img
                        src={entry.content}
                        alt=""
                        className="max-h-40 rounded-lg border border-border/40 object-contain"
                      />
                      <p className="mt-2 text-[11px] text-muted-foreground">
                        {shelfImageSize(entry.image_width, entry.image_height)}
                      </p>
                    </div>
                  ) : (
                    <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words font-sans text-[13px] leading-relaxed text-foreground/90">
                      {previewLines(entry.content, 6)}
                    </pre>
                  )}
                  <div className="flex shrink-0 gap-1">
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8"
                      title={entry.pinned ? "取消固定" : "固定"}
                      onClick={(event) => {
                        event.stopPropagation();
                        void api.pinClipboardEntry(entry.id, !entry.pinned).then(() => load());
                      }}
                    >
                      <Pin className={cn("h-3.5 w-3.5", entry.pinned && "text-primary")} />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8 text-destructive"
                      title="删除"
                      onClick={(event) => {
                        event.stopPropagation();
                        void api.deleteClipboardEntry(entry.id).then(() => load());
                      }}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))
        )}
      </div>
    </div>
  );
}
