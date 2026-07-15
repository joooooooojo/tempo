import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronLeft,
  ChevronRight,
  Copy,
  Loader2,
  Maximize2,
  Pin,
  Search,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { AppIcon } from "@/components/AppIcon";
import { ImagePreviewDialog } from "@/components/ImagePreviewDialog";
import {
  clipboardKindLabel,
  clipboardSourceLabel,
  shelfImageSize,
} from "@/components/clipboard/ShelfCard";
import { Button } from "@/components/ui/button";
import { DataTable } from "@/components/ui/data-table";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import { cn, formatRelativeTime, previewLines } from "@/lib/utils";
import type { ClipboardEntry } from "@/types";

const PAGE_SIZE = 15;

export function ClipboardPage() {
  const [entries, setEntries] = useState<ClipboardEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [previewEntry, setPreviewEntry] = useState<ClipboardEntry | null>(null);
  const queryRef = useRef(query);
  const pageRef = useRef(page);
  const clipboardUpdateTimerRef = useRef<number | null>(null);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const rangeStart = total === 0 ? 0 : (page - 1) * PAGE_SIZE + 1;
  const rangeEnd = Math.min(page * PAGE_SIZE, total);

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  useEffect(() => {
    pageRef.current = page;
  }, [page]);

  const loadPage = useCallback(
    async (nextPage: number, search?: string, showLoading = true) => {
      const normalizedPage = Math.max(1, nextPage);
      if (showLoading) {
        setLoading(true);
      }

      try {
        const result = await api.getClipboardHistory(
          search,
          PAGE_SIZE,
          (normalizedPage - 1) * PAGE_SIZE
        );
        const lastPage = Math.max(1, Math.ceil(result.total / PAGE_SIZE));

        if (result.total > 0 && normalizedPage > lastPage) {
          setPage(lastPage);
          pageRef.current = lastPage;
          await loadPage(lastPage, search, showLoading);
          return;
        }

        setEntries(result.entries);
        setTotal(result.total);
        setPage(normalizedPage);
        pageRef.current = normalizedPage;
      } finally {
        if (showLoading) {
          setLoading(false);
        }
      }
    },
    []
  );

  const reload = useCallback(
    async (showLoading = false) => {
      await loadPage(pageRef.current, queryRef.current || undefined, showLoading);
    },
    [loadPage]
  );

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadPage(1, query || undefined);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [loadPage, query]);

  useEffect(() => {
    const unlisten = listen("clipboard-update", () => {
      if (clipboardUpdateTimerRef.current) {
        window.clearTimeout(clipboardUpdateTimerRef.current);
      }
      clipboardUpdateTimerRef.current = window.setTimeout(() => {
        clipboardUpdateTimerRef.current = null;
        void reload(false);
      }, 160);
    });
    return () => {
      if (clipboardUpdateTimerRef.current) {
        window.clearTimeout(clipboardUpdateTimerRef.current);
        clipboardUpdateTimerRef.current = null;
      }
      void unlisten.then((fn) => fn());
    };
  }, [reload]);

  const copyEntry = async (entry: ClipboardEntry) => {
    try {
      await api.copyClipboardEntry(entry.id);
      if (pageRef.current === 1) {
        setEntries((current) => {
          const nextEntry = { ...entry, created_at: new Date().toISOString() };
          return [nextEntry, ...current.filter((item) => item.id !== entry.id)].slice(0, PAGE_SIZE);
        });
      }
      toast.success("已复制");
      void reload(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    }
  };

  const togglePinned = async (entry: ClipboardEntry) => {
    const nextPinned = !entry.pinned;
    setEntries((current) =>
      current.map((item) => (item.id === entry.id ? { ...item, pinned: nextPinned } : item))
    );
    try {
      await api.pinClipboardEntry(entry.id, nextPinned);
      void reload(false);
    } catch (error) {
      setEntries((current) =>
        current.map((item) => (item.id === entry.id ? { ...item, pinned: entry.pinned } : item))
      );
      toast.error(error instanceof Error ? error.message : "操作失败");
    }
  };

  const deleteEntry = async (entry: ClipboardEntry) => {
    setEntries((current) => current.filter((item) => item.id !== entry.id));
    setTotal((current) => Math.max(0, current - 1));
    try {
      await api.deleteClipboardEntry(entry.id);
      void reload(false);
    } catch (error) {
      setEntries((current) => [entry, ...current]);
      setTotal((current) => current + 1);
      toast.error(error instanceof Error ? error.message : "删除失败");
    }
  };

  const goToPage = (nextPage: number) => {
    const target = Math.min(Math.max(1, nextPage), totalPages);
    if (target === page && !loading) return;
    void loadPage(target, queryRef.current || undefined);
  };

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-6xl flex-1 flex-col gap-3">
      <div className="flex shrink-0 flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索"
            className="h-9 border-0 pl-9 glass-subtle"
          />
        </div>
      </div>

      <DataTable
        loading={loading}
        loadingContent={
          <>
            <Loader2 className="h-4 w-4 animate-spin" />
            加载中...
          </>
        }
        empty={entries.length === 0}
        emptyContent="还没有剪贴记录，复制文字或截图试试吧"
        footer={
          <TablePagination
            page={page}
            totalPages={totalPages}
            rangeStart={rangeStart}
            rangeEnd={rangeEnd}
            total={total}
            onPageChange={goToPage}
          />
        }
        scrollAreaLabel="剪贴板历史"
      >
        <Table className="w-full  table-fixed border-collapse text-left">
          <colgroup>
            <col />
            <col />
            <col />
            <col />
            <col />
            <col className="w-[140px]" />
          </colgroup>
          <TableHeader className="sticky top-0 z-10 bg-background/90 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground backdrop-blur supports-[backdrop-filter]:bg-background/75">
            <TableRow className="border-b border-border/55 hover:bg-transparent">
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">类型</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">内容</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">来源</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">时间</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">详情</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-2 py-2 text-muted-foreground">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {entries.map((entry) => (
              <ClipboardTableRow
                key={entry.id}
                entry={entry}
                onCopy={() => void copyEntry(entry)}
                onDelete={() => void deleteEntry(entry)}
                onPreview={() => setPreviewEntry(entry)}
                onTogglePinned={() => void togglePinned(entry)}
              />
            ))}
          </TableBody>
        </Table>
      </DataTable>

      <ImagePreviewDialog
        image={
          previewEntry
            ? { src: previewEntry.content, alt: clipboardSourceLabel(previewEntry) }
            : null
        }
        onOpenChange={(open) => {
          if (!open) setPreviewEntry(null);
        }}
      />
    </div>
  );
}

function ClipboardTableRow({
  entry,
  onCopy,
  onDelete,
  onPreview,
  onTogglePinned,
}: {
  entry: ClipboardEntry;
  onCopy: () => void;
  onDelete: () => void;
  onPreview: () => void;
  onTogglePinned: () => void;
}) {
  const isImage = entry.kind === "image";
  const sourceLabel = clipboardSourceLabel(entry);
  const detailLabel = isImage
    ? shelfImageSize(entry.image_width, entry.image_height).replace(/\s×\s/g, "×")
    : `${Array.from(entry.content).length} 字符`;

  return (
    <TableRow
      className={cn(
        "h-[52px] border-b border-border/45 text-[12px] transition-colors last:border-b-0 hover:bg-foreground/[0.025]",
        entry.pinned && "bg-primary/[0.035]"
      )}
    >
      <TableCell className="whitespace-nowrap px-3 py-2 align-middle">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              "font-semibold",
              isImage ? "text-amber-700 dark:text-amber-300" : "text-emerald-600 dark:text-emerald-300"
            )}
          >
            {clipboardKindLabel(entry.kind)}
          </span>
          {entry.pinned && <Pin className="h-3 w-3 text-primary" />}
        </div>
      </TableCell>

      <TableCell className="max-w-0 px-3 py-2 align-middle">
        {isImage ? (
          <button
            type="button"
            className="group inline-flex max-w-full items-center rounded-md text-left transition hover:text-primary"
            title="预览图片"
            onClick={onPreview}
          >
            <span className="relative flex h-10 w-[72px] shrink-0 items-center justify-center overflow-hidden rounded bg-background/72 ring-1 ring-border/60">
              <img src={entry.content} alt="" loading="lazy" className="h-full w-full object-contain" />
              <span className="absolute right-0.5 top-0.5 rounded bg-background/80 p-0.5 opacity-0 ring-1 ring-border/60 transition group-hover:opacity-100">
                <Maximize2 className="h-2.5 w-2.5" />
              </span>
            </span>
          </button>
        ) : (
          <pre className="m-0 block max-w-full truncate font-sans text-[12px] leading-[17px] text-foreground/88">
            {previewLines(entry.content, 1)}
          </pre>
        )}
      </TableCell>

      <TableCell className="px-3 py-2 align-middle">
        <div className="flex min-w-0 items-center gap-1.5">
          {entry.source_app && (
            <AppIcon
              name={entry.source_app}
              iconDataUrl={entry.source_icon_data_url}
              size="xs"
            />
          )}
          <span className="truncate text-muted-foreground" title={sourceLabel}>
            {sourceLabel}
          </span>
        </div>
      </TableCell>

      <TableCell className="whitespace-nowrap px-3 py-2 align-middle text-muted-foreground">
        {formatRelativeTime(entry.created_at)}
      </TableCell>

      <TableCell className="whitespace-nowrap px-3 py-2 align-middle text-muted-foreground">{detailLabel}</TableCell>

      <TableCell className="px-2 py-2 align-middle">
        <div className="flex gap-1">
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-primary"
            title="复制"
            onClick={onCopy}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8"
            title={entry.pinned ? "取消固定" : "固定"}
            onClick={onTogglePinned}
          >
            <Pin className={cn("h-3.5 w-3.5", entry.pinned && "text-primary")} />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-destructive"
            title="删除"
            onClick={onDelete}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </TableCell>
    </TableRow>
  );
}

function TablePagination({
  page,
  totalPages,
  rangeStart,
  rangeEnd,
  total,
  onPageChange,
}: {
  page: number;
  totalPages: number;
  rangeStart: number;
  rangeEnd: number;
  total: number;
  onPageChange: (page: number) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-t border-border/55 bg-foreground/[0.018] px-3 py-2">
      <span className="text-[12px] text-muted-foreground">
        显示 {rangeStart}-{rangeEnd}，共 {total} 条
      </span>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-8 px-2.5 text-[12px]"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
          上一页
        </Button>
        <span className="min-w-16 text-center text-[12px] font-medium text-muted-foreground">
          {page} / {totalPages}
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-8 px-2.5 text-[12px]"
          disabled={page >= totalPages}
          onClick={() => onPageChange(page + 1)}
        >
          下一页
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}
