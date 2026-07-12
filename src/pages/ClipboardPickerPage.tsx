import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ClipboardList, MoreHorizontal, Search } from "lucide-react";
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
import { useAuxiliaryWindowShell, useShelfBlurClose } from "@/hooks/useAuxiliaryWindow";
import { api } from "@/lib/api";
import type { ClipboardEntry } from "@/types";

const CLIPBOARD_LIMIT = 200;

export function ClipboardPickerPage() {
  const [entries, setEntries] = useState<ClipboardEntry[]>([]);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [copying, setCopying] = useState(false);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const queryRef = useRef(query);

  useAuxiliaryWindowShell("shelf-picker-window");
  useShelfBlurClose("clipboard-picker:open", copying);

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  const load = useCallback(async (search?: string) => {
    const next = await api.getClipboardHistory(search, CLIPBOARD_LIMIT);
    setEntries(next);
    setSelectedIndex(0);
  }, []);

  useEffect(() => {
    void load();

    const unlistenUpdate = listen("clipboard-update", () => {
      void load(queryRef.current || undefined);
    });
    const unlistenOpen = listen("clipboard-picker:open", () => {
      setQuery("");
      setSearchOpen(false);
      void load();
    });

    return () => {
      void unlistenUpdate.then((fn) => fn());
      void unlistenOpen.then((fn) => fn());
    };
  }, [load]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void load(query || undefined);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [load, query]);

  const copyEntry = useCallback(async (entry: ClipboardEntry) => {
    setCopying(true);
    try {
      await api.copyClipboardEntry(entry.id);
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
      if (entries.length === 0) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        setSelectedIndex((index) => Math.min(index + 1, entries.length - 1));
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        setSelectedIndex((index) => Math.max(index - 1, 0));
      } else if (event.key === "Enter") {
        event.preventDefault();
        const entry = entries[selectedIndex];
        if (entry) void copyEntry(entry);
      } else if (event.key === "Escape") {
        event.preventDefault();
        void getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [copyEntry, entries, selectedIndex]);

  useEffect(() => {
    const node = scrollerRef.current?.children[selectedIndex] as HTMLElement | undefined;
    node?.scrollIntoView({ behavior: "smooth", inline: "center", block: "nearest" });
  }, [selectedIndex, entries.length]);

  return (
    <div className="shelf-picker-page shelf-picker-page--full">
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
            <ClipboardList className="h-4 w-4 text-primary" />
            <span>剪贴板</span>
          </div>
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
              placeholder="搜索历史..."
              className="shelf-picker-search__input"
            />
          </div>
        )}

        <div className="shelf-picker-track" ref={scrollerRef}>
          {entries.length === 0 ? (
            <div className="shelf-picker-empty">暂无剪贴记录</div>
          ) : (
            entries.map((entry, index) => (
              <ShelfCard
                key={entry.id}
                selected={index === selectedIndex}
                headerLabel={clipboardKindLabel(entry.kind)}
                headerTone={clipboardHeaderTone(entry.kind)}
                timeLabel={shelfTimeLabel(entry.created_at)}
                sourceApp={clipboardSourceLabel(entry)}
                content={entry.kind === "image" ? "" : entry.content}
                imageSrc={entry.kind === "image" ? entry.content : null}
                footer={
                  entry.kind === "image"
                    ? shelfImageSize(entry.image_width, entry.image_height)
                    : shelfCharCount(entry.content)
                }
                onClick={() => setSelectedIndex(index)}
                onDoubleClick={() => void copyEntry(entry)}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}
