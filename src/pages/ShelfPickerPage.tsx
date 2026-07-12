import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ClipboardList, MoreHorizontal, Plus, Search, TextQuote } from "lucide-react";
import { toast } from "sonner";
import {
  clipboardHeaderTone,
  clipboardKindLabel,
  clipboardSourceLabel,
  ShelfCard,
  shelfCharCount,
  shelfImageSize,
  shelfTimeLabel,
} from "@/components/clipboard/ShelfCard";
import { useAuxiliaryWindowShell } from "@/hooks/useAuxiliaryWindow";
import { api } from "@/lib/api";
import type { ClipboardEntry, Snippet } from "@/types";

const CLIPBOARD_LIMIT = 200;

type ShelfTab = "clipboard" | "snippets";

function isShelfTab(value: unknown): value is ShelfTab {
  return value === "clipboard" || value === "snippets";
}

function filterClipboard(entries: ClipboardEntry[], query: string) {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter(
    (entry) => entry.kind === "text" && entry.content.toLowerCase().includes(needle)
  );
}

function filterSnippets(items: Snippet[], query: string) {
  const needle = query.trim().toLowerCase();
  if (!needle) return items;
  return items.filter(
    (snippet) =>
      snippet.title.toLowerCase().includes(needle) ||
      snippet.content.toLowerCase().includes(needle) ||
      snippet.tags.some((tag) => tag.toLowerCase().includes(needle))
  );
}

const MemoShelfCard = memo(ShelfCard);

export function ShelfPickerPage() {
  const [tab, setTab] = useState<ShelfTab>("clipboard");
  const [clipboardCache, setClipboardCache] = useState<ClipboardEntry[]>([]);
  const [snippetsCache, setSnippetsCache] = useState<Snippet[]>([]);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [clipboardIndex, setClipboardIndex] = useState(0);
  const [snippetsIndex, setSnippetsIndex] = useState(0);
  const clipboardScrollerRef = useRef<HTMLDivElement>(null);
  const snippetsScrollerRef = useRef<HTMLDivElement>(null);
  const searchInlineRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const tabRef = useRef(tab);

  const entries = useMemo(
    () => filterClipboard(clipboardCache, query),
    [clipboardCache, query]
  );
  const snippets = useMemo(() => filterSnippets(snippetsCache, query), [snippetsCache, query]);

  useAuxiliaryWindowShell("shelf-picker-window");

  const hideShelf = useCallback(() => {
    void api.hideShelfPicker();
  }, []);

  useEffect(() => {
    tabRef.current = tab;
  }, [tab]);

  useEffect(() => {
    setClipboardIndex(0);
    setSnippetsIndex(0);
  }, [query]);

  useEffect(() => {
    setClipboardIndex((index) => Math.min(index, Math.max(entries.length - 1, 0)));
  }, [entries.length]);

  useEffect(() => {
    setSnippetsIndex((index) => Math.min(index, Math.max(snippets.length - 1, 0)));
  }, [snippets.length]);

  useEffect(() => {
    if (!searchOpen) return;
    const timer = window.setTimeout(() => searchInputRef.current?.focus(), 60);
    return () => window.clearTimeout(timer);
  }, [searchOpen]);

  const closeSearch = useCallback(() => {
    setSearchOpen(false);
    setQuery("");
  }, []);

  const toggleSearch = useCallback(() => {
    setSearchOpen((open) => {
      if (open) {
        setQuery("");
      }
      return !open;
    });
  }, []);

  useEffect(() => {
    if (!searchOpen) return;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (searchInlineRef.current?.contains(target)) return;
      closeSearch();
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  }, [closeSearch, searchOpen]);

  const loadClipboard = useCallback(async (resetIndex = false) => {
    const page = await api.getClipboardHistory(undefined, CLIPBOARD_LIMIT);
    setClipboardCache(page.entries);
    if (resetIndex) {
      setClipboardIndex(0);
    } else {
      setClipboardIndex((index) => Math.min(index, Math.max(page.entries.length - 1, 0)));
    }
  }, []);

  const loadSnippets = useCallback(async (resetIndex = false) => {
    const next = await api.getSnippets();
    setSnippetsCache(next);
    if (resetIndex) {
      setSnippetsIndex(0);
    } else {
      setSnippetsIndex((index) => Math.min(index, Math.max(next.length - 1, 0)));
    }
  }, []);

  const resetAndOpen = useCallback(
    (nextTab: ShelfTab) => {
      setTab(nextTab);
      setQuery("");
      setSearchOpen(true);
      void loadClipboard(true);
      void loadSnippets(true);
    },
    [loadClipboard, loadSnippets]
  );

  useEffect(() => {
    void loadClipboard(true);
    void loadSnippets(true);

    const unlistenClipboard = listen("clipboard-update", () => {
      void loadClipboard();
    });
    const unlistenSnippets = listen("snippets-update", () => {
      void loadSnippets();
    });
    const unlistenOpen = listen<{ tab?: string }>("shelf-picker:open", (event) => {
      const nextTab = isShelfTab(event.payload.tab) ? event.payload.tab : "clipboard";
      resetAndOpen(nextTab);
    });
    const unlistenActivate = listen<{ tab?: string }>("shelf-picker:activate", (event) => {
      const nextTab = isShelfTab(event.payload.tab) ? event.payload.tab : "clipboard";
      if (nextTab === tabRef.current) {
        hideShelf();
        return;
      }
      setTab(nextTab);
    });

    return () => {
      void unlistenClipboard.then((fn) => fn());
      void unlistenSnippets.then((fn) => fn());
      void unlistenOpen.then((fn) => fn());
      void unlistenActivate.then((fn) => fn());
    };
  }, [loadClipboard, loadSnippets, resetAndOpen]);

  const copyEntry = useCallback(async (entry: ClipboardEntry) => {
    try {
      await api.copyClipboardEntry(entry.id);
      setClipboardCache((current) => {
        const nextEntry = { ...entry, created_at: new Date().toISOString() };
        return [nextEntry, ...current.filter((item) => item.id !== entry.id)];
      });
      setClipboardIndex(0);
      await api.hideShelfPicker();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    }
  }, []);

  const copySnippet = useCallback(async (snippet: Snippet) => {
    try {
      await api.copySnippetToClipboard(snippet.id);
      toast.success("已复制");
      await api.hideShelfPicker();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    }
  }, []);

  const itemCount = tab === "clipboard" ? entries.length : snippets.length;

  const scrollSelectedCardIntoView = useCallback(
    (container: HTMLDivElement | null, index: number) => {
      const node = container?.children[index] as HTMLElement | undefined;
      node?.scrollIntoView({ behavior: "auto", inline: "nearest", block: "nearest" });
    },
    []
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (itemCount === 0) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        if (tab === "clipboard") {
          const next = Math.min(clipboardIndex + 1, entries.length - 1);
          setClipboardIndex(next);
          requestAnimationFrame(() =>
            scrollSelectedCardIntoView(clipboardScrollerRef.current, next)
          );
        } else {
          const next = Math.min(snippetsIndex + 1, snippets.length - 1);
          setSnippetsIndex(next);
          requestAnimationFrame(() =>
            scrollSelectedCardIntoView(snippetsScrollerRef.current, next)
          );
        }
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        if (tab === "clipboard") {
          const next = Math.max(clipboardIndex - 1, 0);
          setClipboardIndex(next);
          requestAnimationFrame(() =>
            scrollSelectedCardIntoView(clipboardScrollerRef.current, next)
          );
        } else {
          const next = Math.max(snippetsIndex - 1, 0);
          setSnippetsIndex(next);
          requestAnimationFrame(() =>
            scrollSelectedCardIntoView(snippetsScrollerRef.current, next)
          );
        }
      } else if (event.key === "Enter") {
        event.preventDefault();
        if (tab === "clipboard") {
          const entry = entries[clipboardIndex];
          if (entry) void copyEntry(entry);
        } else {
          const snippet = snippets[snippetsIndex];
          if (snippet) void copySnippet(snippet);
        }
      } else if (event.key === "Escape") {
        event.preventDefault();
        if (searchOpen) {
          closeSearch();
          return;
        }
        hideShelf();
      } else if (event.key === "Tab" && !event.shiftKey && !searchOpen) {
        event.preventDefault();
        setTab((current) => (current === "clipboard" ? "snippets" : "clipboard"));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    clipboardIndex,
    closeSearch,
    copyEntry,
    copySnippet,
    entries,
    hideShelf,
    itemCount,
    searchOpen,
    snippets,
    snippetsIndex,
    scrollSelectedCardIntoView,
    tab,
  ]);

  return (
    <div className="shelf-picker-page shelf-picker-page--full">
      <div className="shelf-picker-panel">
        <div className="shelf-picker-toolbar">
          <div
            ref={searchInlineRef}
            className={`shelf-picker-search-inline${searchOpen ? " shelf-picker-search-inline--open" : ""}`}
          >
            <button
              type="button"
              className="shelf-picker-search-inline__trigger"
              title="搜索"
              aria-label="搜索"
              aria-expanded={searchOpen}
              onClick={toggleSearch}
            >
              <Search className="h-4 w-4" />
            </button>
            <input
              ref={searchInputRef}
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={tab === "clipboard" ? "搜索历史..." : "搜索短语..."}
              className="shelf-picker-search-inline__input"
              tabIndex={searchOpen ? 0 : -1}
            />
          </div>

          <div className="shelf-picker-tabs" role="tablist" aria-label="内容切换">
            <button
              type="button"
              role="tab"
              aria-selected={tab === "clipboard"}
              className={`shelf-picker-tab${tab === "clipboard" ? " shelf-picker-tab--active" : ""}`}
              onClick={() => setTab("clipboard")}
            >
              <ClipboardList className="h-3.5 w-3.5" />
              <span>剪贴板</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "snippets"}
              className={`shelf-picker-tab${tab === "snippets" ? " shelf-picker-tab--active" : ""}`}
              onClick={() => setTab("snippets")}
            >
              <TextQuote className="h-3.5 w-3.5" />
              <span>快捷短语</span>
            </button>
          </div>

          <div className="shelf-picker-toolbar-actions">
            {tab === "snippets" && (
              <button type="button" className="shelf-picker-icon-btn" title="新建" disabled>
                <Plus className="h-4 w-4" />
              </button>
            )}
            <button type="button" className="shelf-picker-icon-btn" title="更多" disabled>
              <MoreHorizontal className="h-4 w-4" />
            </button>
          </div>
        </div>

        <div className="shelf-picker-track-host">
          <div
            className="shelf-picker-track"
            ref={clipboardScrollerRef}
            hidden={tab !== "clipboard"}
            aria-hidden={tab !== "clipboard"}
          >
            {entries.length === 0 ? (
              <div className="shelf-picker-empty">暂无剪贴记录</div>
            ) : (
              entries.map((entry, index) => (
                <MemoShelfCard
                  key={entry.id}
                  selected={index === clipboardIndex}
                  headerLabel={clipboardKindLabel(entry.kind)}
                  headerTone={clipboardHeaderTone(entry.kind)}
                  timeLabel={shelfTimeLabel(entry.created_at)}
                  sourceApp={clipboardSourceLabel(entry)}
                  sourceAppIcon={entry.source_icon_data_url}
                  content={entry.kind === "image" ? "" : entry.content}
                  imageSrc={entry.kind === "image" ? entry.content : null}
                  footer={
                    entry.kind === "image"
                      ? shelfImageSize(entry.image_width, entry.image_height)
                      : shelfCharCount(entry.content)
                  }
                  onClick={() => setClipboardIndex(index)}
                  onDoubleClick={() => void copyEntry(entry)}
                  title="双击复制到剪贴板并置顶"
                />
              ))
            )}
          </div>

          <div
            className="shelf-picker-track"
            ref={snippetsScrollerRef}
            hidden={tab !== "snippets"}
            aria-hidden={tab !== "snippets"}
          >
            {snippets.length === 0 ? (
              <div className="shelf-picker-empty">还没有快捷短语</div>
            ) : (
              snippets.map((snippet, index) => (
                <MemoShelfCard
                  key={snippet.id}
                  selected={index === snippetsIndex}
                  headerLabel="短语"
                  headerTone="snippet"
                  timeLabel={shelfTimeLabel(snippet.updated_at)}
                  sourceApp={snippet.title}
                  content={snippet.content}
                  footer={
                    snippet.tags.length > 0
                      ? snippet.tags.join(" · ")
                      : shelfCharCount(snippet.content)
                  }
                  onClick={() => setSnippetsIndex(index)}
                  onDoubleClick={() => void copySnippet(snippet)}
                />
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
