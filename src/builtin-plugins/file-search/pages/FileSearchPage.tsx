import {
  startTransition,
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Download,
  ExternalLink,
  FileIcon,
  FolderOpen,
  Loader2,
  RefreshCw,
  Search,
  TriangleAlert,
} from "lucide-react";
import { toast } from "sonner";
import { useOptionalMainPanelAppBarChrome } from "@/apps/appBarChrome";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Progress,
  ProgressLabel,
  ProgressValue,
} from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type {
  FileSearchEngineProgress,
  FileSearchItem,
  FileSearchPreviewMeta,
  FileSearchStatus,
} from "@/types";
import { FileSearchPreview, resolvePreviewKind } from "@/builtin-plugins/file-search/components/FileSearchPreview";
import {
  FILE_SEARCH_CATEGORIES,
  FILE_SEARCH_SORTS,
  truncateMiddle,
  type FileSearchCategoryId,
  type FileSearchSortId,
} from "@/builtin-plugins/file-search/pages/fileSearchShared";
import "@/builtin-plugins/file-search/styles/file-search.css";

const DEBOUNCE_MS = 350;
const PAGE_LIMIT = 100;
const LOAD_MORE_THRESHOLD_PX = 120;
const ENGINE_PROGRESS_EVENT = "file-search:engine-progress";

const SORT_SELECT_ITEMS = FILE_SEARCH_SORTS.map((item) => ({
  value: item.id,
  label: item.label,
}));

const STAGE_LABELS: Record<string, string> = {
  download_everything: "正在下载 Everything…",
  download_es: "正在下载 ES…",
  extract: "正在解压…",
  start: "正在启动 Everything…",
  download_fd: "正在下载 fd…",
  done: "完成",
};

function engineStageLabel(progress: FileSearchEngineProgress | null): string {
  if (!progress) return "准备中…";
  if (progress.label?.trim()) return progress.label.trim();
  return STAGE_LABELS[progress.stage] ?? "处理中…";
}

function formatDownloadedBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Stable app-bar field: owns draft text so chrome updates don't remount on every keystroke. */
function FileSearchAppBarSearch({
  searching,
  onDraftChange,
  onSubmit,
  inputRef,
}: {
  searching: boolean;
  onDraftChange: (value: string) => void;
  onSubmit: () => void;
  inputRef: RefObject<HTMLInputElement | null>;
}) {
  const [value, setValue] = useState("");

  return (
    <div className="file-search-app-bar">
      <div className="file-search-app-bar__field">
        <Search className="file-search-app-bar__icon" aria-hidden />
        <Input
          ref={inputRef}
          value={value}
          onChange={(event) => {
            const next = event.target.value;
            setValue(next);
            onDraftChange(next);
          }}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            event.stopPropagation();
            onSubmit();
          }}
          placeholder="全盘搜索"
          className="file-search-app-bar__input"
          autoFocus
          aria-label="全盘搜索"
        />
      </div>
      <Button
        type="button"
        size="lg"
        className="file-search-app-bar__submit"
        onClick={onSubmit}
        aria-busy={searching}
      >
        {searching ? (
          <Loader2 className="h-4 w-4 animate-spin" />
        ) : (
          <Search className="h-4 w-4" />
        )}
        搜索
      </Button>
    </div>
  );
}

export function FileSearchPage() {
  const appBarChrome = useOptionalMainPanelAppBarChrome();
  const setAppBarChrome = appBarChrome?.setChrome;
  const [status, setStatus] = useState<FileSearchStatus | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [ensuring, setEnsuring] = useState(false);
  const [engineProgress, setEngineProgress] = useState<FileSearchEngineProgress | null>(null);
  const [query, setQuery] = useState("");
  const [draftQuery, setDraftQuery] = useState("");
  const [category, setCategory] = useState<FileSearchCategoryId>("all");
  const [sort, setSort] = useState<FileSearchSortId>("mtime_desc");
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const [items, setItems] = useState<FileSearchItem[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<FileSearchPreviewMeta | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const requestId = useRef(0);
  const listRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const itemsRef = useRef<FileSearchItem[]>([]);
  const hasMoreRef = useRef(false);
  const loadingMoreRef = useRef(false);
  const searchingRef = useRef(false);
  const draftQueryRef = useRef("");
  const queryKeyRef = useRef({ query: "", category: "all" as FileSearchCategoryId, sort: "mtime_desc" as FileSearchSortId });

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);
  useEffect(() => {
    hasMoreRef.current = hasMore;
  }, [hasMore]);
  useEffect(() => {
    loadingMoreRef.current = loadingMore;
  }, [loadingMore]);
  useEffect(() => {
    searchingRef.current = searching;
  }, [searching]);
  useEffect(() => {
    draftQueryRef.current = draftQuery;
  }, [draftQuery]);
  useEffect(() => {
    queryKeyRef.current = { query, category, sort };
  }, [query, category, sort]);

  const loadStatus = useCallback(async () => {
    setStatusLoading(true);
    try {
      const next = await api.fileSearchStatus();
      setStatus(next);
    } catch (err) {
      setStatus({
        ready: false,
        engine: null,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  useEffect(() => {
    if (!ensuring) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listen<FileSearchEngineProgress>(ENGINE_PROGRESS_EVENT, (event) => {
      if (!cancelled) setEngineProgress(event.payload);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [ensuring]);

  const ensureEngine = async () => {
    if (ensuring) return;
    setEnsuring(true);
    setEngineProgress(null);
    setError(null);
    try {
      const next = await api.fileSearchEnsureEngine();
      setStatus(next);
      if (next.ready) {
        toast.success(next.message ?? "搜索引擎已就绪");
      } else {
        toast.error(next.message ?? "启用失败");
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(message);
    } finally {
      setEnsuring(false);
      setEngineProgress(null);
    }
  };

  const runSearch = useCallback(
    async (nextQuery: string, nextCategory: FileSearchCategoryId, nextSort: FileSearchSortId) => {
      const trimmed = nextQuery.trim();
      if (!status?.ready) return;

      const current = ++requestId.current;
      setSearching(true);
      setLoadingMore(false);
      setError(null);
      // Keep previous rows visible until the new page arrives — clearing first
      // made the list flash empty (共 0 项) while IPC was still in flight.
      try {
        const result = await api.fileSearchQuery({
          query: trimmed,
          category: nextCategory,
          sort: nextSort,
          limit: PAGE_LIMIT,
          offset: 0,
        });
        if (current !== requestId.current) return;
        startTransition(() => {
          setItems(result.items);
          setTotal(result.total);
          setHasMore(result.hasMore);
          setSelectedIndex(0);
        });
      } catch (err) {
        if (current !== requestId.current) return;
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        setItems([]);
        setTotal(0);
        setHasMore(false);
      } finally {
        if (current === requestId.current) setSearching(false);
      }
    },
    [status?.ready],
  );

  const loadMore = useCallback(async () => {
    if (!status?.ready) return;
    if (searchingRef.current || loadingMoreRef.current || !hasMoreRef.current) return;

    const { query: q, category: cat, sort: s } = queryKeyRef.current;
    const trimmed = q.trim();

    const offset = itemsRef.current.length;
    const current = requestId.current;
    setLoadingMore(true);
    try {
      const result = await api.fileSearchQuery({
        query: trimmed,
        category: cat,
        sort: s,
        limit: PAGE_LIMIT,
        offset,
      });
      if (current !== requestId.current) return;
      const incoming = result.items;
      const existing = new Set(itemsRef.current.map((item) => item.path));
      const unique = incoming.filter((item) => !existing.has(item.path));
      startTransition(() => {
        if (unique.length > 0) {
          setItems((prev) => [...prev, ...unique]);
        }
        setTotal((prev) => {
          const next = result.total;
          if (prev == null) return next;
          return Math.max(prev, next);
        });
        // Empty / all-duplicate page → stop to avoid a scroll loop.
        setHasMore(result.hasMore && unique.length > 0);
      });
    } catch (err) {
      if (current !== requestId.current) return;
      const message = err instanceof Error ? err.message : String(err);
      toast.error(message);
      setHasMore(false);
    } finally {
      if (current === requestId.current) setLoadingMore(false);
    }
  }, [status?.ready]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setQuery(draftQuery);
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [draftQuery]);

  useEffect(() => {
    void runSearch(query, category, sort);
  }, [query, category, sort, runSearch]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onScroll = () => {
      const { scrollTop, clientHeight, scrollHeight } = viewport;
      if (scrollTop + clientHeight >= scrollHeight - LOAD_MORE_THRESHOLD_PX) {
        void loadMore();
      }
    };

    viewport.addEventListener("scroll", onScroll, { passive: true });
    // If the first page doesn't fill the viewport, keep loading.
    onScroll();
    return () => viewport.removeEventListener("scroll", onScroll);
  }, [loadMore, items.length, searching, hasMore]);

  const selected = items[selectedIndex] ?? null;

  useEffect(() => {
    if (!previewEnabled || !selected) {
      setPreview(null);
      setPreviewLoading(false);
      setPreviewError(null);
      return;
    }
    let cancelled = false;
    setPreviewLoading(true);
    setPreviewError(null);
    api.fileSearchPreviewMeta(selected.path)
      .then((meta) => {
        if (cancelled) return;
        setPreview(meta);
        setPreviewLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        setPreview({
          path: selected.path,
          name: selected.name,
          size: selected.size,
          modifiedAt: selected.modifiedAt,
          isDir: selected.isDir,
          extension: selected.extension,
          previewKind: resolvePreviewKind(null, selected),
        });
        setPreviewLoading(false);
        if (resolvePreviewKind(null, selected) !== "none") {
          setPreviewError(message.trim() || "无法读取预览信息");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [selected, previewEnabled]);

  useEffect(() => {
    const node = listRef.current?.querySelector<HTMLElement>(
      `[data-file-search-index="${selectedIndex}"]`,
    );
    node?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const openSelected = async () => {
    if (!selected) return;
    try {
      await api.fileSearchOpen(selected.path);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const revealSelected = async () => {
    if (!selected) return;
    try {
      await api.fileSearchReveal(selected.path);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (!items.length) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((prev) => Math.min(items.length - 1, prev + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      void openSelected();
    }
  };

  const submitSearch = useCallback(() => {
    const next = draftQueryRef.current;
    // Bump request id so any in-flight auto-search is ignored.
    if (next === query) {
      void runSearch(next, category, sort);
    } else {
      setQuery(next);
    }
  }, [category, query, runSearch, sort]);

  const onDraftChange = useCallback((value: string) => {
    setDraftQuery(value);
  }, []);

  const submitSearchRef = useRef(submitSearch);
  submitSearchRef.current = submitSearch;

  const stableSubmit = useCallback(() => {
    submitSearchRef.current();
  }, []);

  useEffect(() => {
    if (!setAppBarChrome) return;
    if (!status?.ready) {
      setAppBarChrome({});
      return;
    }

    // Omit draftQuery from deps: field owns local text so chrome doesn't rebuild
    // (and re-render the whole main panel) on every keystroke. searching only
    // updates the button spinner; React keeps the field's local state.
    setAppBarChrome({
      leadingGrow: true,
      hideIcon: true,
      leading: (
        <FileSearchAppBarSearch
          searching={searching}
          onDraftChange={onDraftChange}
          onSubmit={stableSubmit}
          inputRef={searchInputRef}
        />
      ),
    });
  }, [onDraftChange, searching, setAppBarChrome, stableSubmit, status?.ready]);

  useEffect(() => {
    return () => setAppBarChrome?.({});
  }, [setAppBarChrome]);

  useEffect(() => {
    if (!status?.ready) return;
    searchInputRef.current?.focus();
  }, [status?.ready]);

  if (statusLoading) {
    return (
      <div className="file-search-shell file-search-shell--center">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        <p className="text-[13px] text-muted-foreground">正在检查搜索引擎…</p>
      </div>
    );
  }

  if (!status?.ready) {
    const hasTotal =
      typeof engineProgress?.total === "number" &&
      engineProgress.total > 0 &&
      typeof engineProgress.percent === "number";
    const progressValue = hasTotal ? Math.min(100, Math.max(0, engineProgress!.percent!)) : null;

    return (
      <div className="file-search-shell file-search-shell--center">
        <div className="file-search-setup">
          <div className="file-search-setup__icon">
            <Search className="h-6 w-6" />
          </div>
          <h2 className="file-search-setup__title">启用全盘文件搜索</h2>
          <p className="file-search-setup__desc">
            {status?.engine === "fd"
              ? "macOS 将按需下载 fd，并以实时扫描方式搜索全盘（无持久索引）。"
              : "Windows 优先复用本机 Everything；若未安装将下载便携版与 ES 命令行。"}
          </p>
          {status?.message ? (
            <p className="file-search-setup__hint">{status.message}</p>
          ) : null}
          {error ? (
            <p className="file-search-setup__error">
              <TriangleAlert className="h-3.5 w-3.5" />
              {error}
            </p>
          ) : null}
          <div className="flex flex-wrap justify-center gap-2">
            <Button type="button" disabled={ensuring} onClick={() => void ensureEngine()}>
              {ensuring ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Download className="h-4 w-4" />
              )}
              {ensuring ? "下载中…" : "下载并启用搜索引擎"}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={ensuring}
              onClick={() => void loadStatus()}
            >
              <RefreshCw className="h-4 w-4" />
              重新检测
            </Button>
          </div>
          {ensuring ? (
            <div
              className={cn(
                "file-search-setup__progress",
                !hasTotal && "file-search-setup__progress--indeterminate",
              )}
            >
              <Progress value={progressValue} className="w-full">
                <ProgressLabel className="text-[12px]">
                  {engineStageLabel(engineProgress)}
                </ProgressLabel>
                <ProgressValue className="text-[11px]">
                  {() => {
                    if (hasTotal) return `${Math.round(progressValue ?? 0)}%`;
                    const stage = engineProgress?.stage ?? "";
                    if (
                      stage === "download_everything" ||
                      stage === "download_es" ||
                      stage === "download_fd"
                    ) {
                      return `已下载 ${formatDownloadedBytes(engineProgress?.current ?? 0)}`;
                    }
                    return "";
                  }}
                </ProgressValue>
              </Progress>
            </div>
          ) : null}
        </div>
      </div>
    );
  }

  return (
    <div className="file-search-shell" onKeyDown={onKeyDown}>
      <div
        className={cn(
          "file-search-body",
          !previewEnabled && "file-search-body--preview-off",
        )}
      >
        <aside className="file-search-cats" aria-label="分类">
          {FILE_SEARCH_CATEGORIES.map((item) => (
            <button
              key={item.id}
              type="button"
              className={cn(
                "file-search-cats__item",
                category === item.id && "file-search-cats__item--active",
              )}
              onClick={() => setCategory(item.id)}
            >
              {item.label}
            </button>
          ))}
        </aside>

        <section className="file-search-list" aria-label="搜索结果">
          {error ? (
            <div className="file-search-empty file-search-empty--error">
              <TriangleAlert className="h-4 w-4" />
              <p>{error}</p>
              <Button type="button" size="sm" variant="outline" onClick={() => void ensureEngine()}>
                重试引擎
              </Button>
            </div>
          ) : items.length === 0 && searching ? (
            <div className="file-search-empty">
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              <p>搜索中…</p>
            </div>
          ) : items.length === 0 && !searching ? (
            <div className="file-search-empty">
              <p>没有匹配结果</p>
            </div>
          ) : (
            <ScrollArea
              className="h-full"
              viewportClassName="file-search-list__scroll"
              viewportRef={viewportRef}
            >
              <div ref={listRef}>
                {items.map((item, index) => {
                  const active = index === selectedIndex;
                  return (
                    <button
                      key={`${item.path}-${index}`}
                      type="button"
                      data-file-search-index={index}
                      className={cn(
                        "file-search-row",
                        active && "file-search-row--active",
                      )}
                      onClick={() => setSelectedIndex(index)}
                      onDoubleClick={() => void openSelected()}
                    >
                      <span className="file-search-row__icon">
                        {item.isDir ? (
                          <FolderOpen className="h-4 w-4" />
                        ) : (
                          <FileIcon className="h-4 w-4" />
                        )}
                      </span>
                      <span className="file-search-row__text">
                        <span className="file-search-row__name">{item.name}</span>
                        <span className="file-search-row__path" title={item.path}>
                          {truncateMiddle(item.path)}
                        </span>
                      </span>
                    </button>
                  );
                })}
                {hasMore || loadingMore ? (
                  <div className="file-search-list__sentinel" aria-hidden={!loadingMore}>
                    {loadingMore ? (
                      <>
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        <span>加载中…</span>
                      </>
                    ) : (
                      <span>向下滚动加载更多</span>
                    )}
                  </div>
                ) : null}
              </div>
            </ScrollArea>
          )}
        </section>

        {previewEnabled ? (
          <aside className="file-search-detail" aria-label="预览">
            {selected ? (
              <div className="file-search-detail__inner">
                <div className="file-search-detail__preview">
                  <FileSearchPreview
                    enabled
                    item={selected}
                    meta={preview}
                    metaLoading={previewLoading}
                    metaError={previewError}
                    onOpen={() => void openSelected()}
                  />
                </div>
                <div className="file-search-detail__actions">
                  <Button type="button" size="sm" onClick={() => void openSelected()}>
                    <ExternalLink className="h-3.5 w-3.5" />
                    打开
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => void revealSelected()}
                  >
                    <FolderOpen className="h-3.5 w-3.5" />
                    打开位置
                  </Button>
                </div>
              </div>
            ) : (
              <div className="file-search-detail__inner">
                <div className="file-search-detail__preview">
                  <FileSearchPreview
                    enabled
                    item={null}
                    meta={null}
                    metaLoading={false}
                    metaError={null}
                    onOpen={() => undefined}
                  />
                </div>
              </div>
            )}
          </aside>
        ) : null}
      </div>

      <footer className="file-search-foot">
        <div className="file-search-foot__sort">
          <Label className="text-[11px] text-muted-foreground">排序</Label>
          <Select
            items={SORT_SELECT_ITEMS}
            value={sort}
            onValueChange={(value) => value && setSort(value as FileSearchSortId)}
          >
            <SelectTrigger className="h-8 w-[140px] text-[12px]">
              <SelectValue>
                {FILE_SEARCH_SORTS.find((s) => s.id === sort)?.label}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {SORT_SELECT_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>
        <div className="file-search-foot__preview">
          <Switch
            id="file-search-preview"
            checked={previewEnabled}
            onCheckedChange={setPreviewEnabled}
          />
          <Label htmlFor="file-search-preview" className="text-[12px] text-muted-foreground">
            文件预览
          </Label>
        </div>
        <p className="file-search-foot__count">
          {searching && items.length === 0
            ? "搜索中…"
            : hasMore && total != null
              ? `已加载 ${items.length} 项`
              : `共 ${total ?? items.length} 项`}
        </p>
      </footer>
    </div>
  );
}
