import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MoreHorizontal, Plus, Search, TextQuote } from "lucide-react";
import { toast } from "sonner";
import { ShelfCard, shelfCharCount, shelfTimeLabel } from "@/components/clipboard/ShelfCard";
import { useAuxiliaryWindowShell, useShelfBlurClose } from "@/hooks/useAuxiliaryWindow";
import { api } from "@/lib/api";
import type { Snippet } from "@/types";

export function SnippetPickerPage() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [copying, setCopying] = useState(false);
  const scrollerRef = useRef<HTMLDivElement>(null);

  useAuxiliaryWindowShell("shelf-picker-window");
  useShelfBlurClose("snippet-picker:open", copying);

  const load = useCallback(async (search?: string) => {
    const next = await api.getSnippets(search);
    setSnippets(next);
    setSelectedIndex(0);
  }, []);

  useEffect(() => {
    void load();
    const unlistenUpdate = listen("snippets-update", () => void load(query || undefined));
    const unlistenOpen = listen("snippet-picker:open", () => {
      setQuery("");
      setSearchOpen(false);
      void load();
    });
    return () => {
      void unlistenUpdate.then((fn) => fn());
      void unlistenOpen.then((fn) => fn());
    };
  }, [load, query]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void load(query || undefined);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [load, query]);

  const copySnippet = useCallback(async (snippet: Snippet) => {
    setCopying(true);
    try {
      await api.copySnippetToClipboard(snippet.id);
      toast.success("已复制");
      await getCurrentWindow().hide();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    } finally {
      setCopying(false);
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (snippets.length === 0) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        setSelectedIndex((index) => Math.min(index + 1, snippets.length - 1));
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        setSelectedIndex((index) => Math.max(index - 1, 0));
      } else if (event.key === "Enter") {
        event.preventDefault();
        const snippet = snippets[selectedIndex];
        if (snippet) void copySnippet(snippet);
      } else if (event.key === "Escape") {
        event.preventDefault();
        void getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [copySnippet, selectedIndex, snippets]);

  useEffect(() => {
    const node = scrollerRef.current?.children[selectedIndex] as HTMLElement | undefined;
    node?.scrollIntoView({ behavior: "smooth", inline: "center", block: "nearest" });
  }, [selectedIndex, snippets.length]);

  return (
    <div className="shelf-picker-page">
      <div className="shelf-picker-panel">
        <div className="shelf-picker-toolbar">
          <button
            type="button"
            className="shelf-picker-icon-btn"
            title="搜索"
            onClick={() => setSearchOpen((open) => !open)}
          >
            <Search className="h-4 w-4" />
          </button>
          <div className="shelf-picker-title">
            <TextQuote className="h-4 w-4 text-primary" />
            <span>快捷短语</span>
          </div>
          <button type="button" className="shelf-picker-icon-btn" title="新建" disabled>
            <Plus className="h-4 w-4" />
          </button>
          <button type="button" className="shelf-picker-icon-btn" title="更多" disabled>
            <MoreHorizontal className="h-4 w-4" />
          </button>
        </div>

        {searchOpen && (
          <div className="shelf-picker-search">
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索短语..."
              className="shelf-picker-search__input"
            />
          </div>
        )}

        <div className="shelf-picker-track" ref={scrollerRef}>
          {snippets.length === 0 ? (
            <div className="shelf-picker-empty">还没有快捷短语</div>
          ) : (
            snippets.map((snippet, index) => (
              <ShelfCard
                key={snippet.id}
                selected={index === selectedIndex}
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
                onClick={() => setSelectedIndex(index)}
                onDoubleClick={() => void copySnippet(snippet)}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}
