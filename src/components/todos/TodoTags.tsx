import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { Tag, X } from "lucide-react";
import { getTagHistory, mergeTagSuggestions, recordTag } from "@/lib/tagHistory";
import { cn } from "@/lib/utils";

const TAG_PALETTE = [
  "bg-sky-500/12 text-sky-700 ring-sky-500/20 dark:text-sky-300",
  "bg-violet-500/12 text-violet-700 ring-violet-500/20 dark:text-violet-300",
  "bg-amber-500/12 text-amber-700 ring-amber-500/20 dark:text-amber-300",
  "bg-rose-500/12 text-rose-700 ring-rose-500/20 dark:text-rose-300",
  "bg-teal-500/12 text-teal-700 ring-teal-500/20 dark:text-teal-300",
  "bg-orange-500/12 text-orange-700 ring-orange-500/20 dark:text-orange-300",
] as const;

const MAX_VISIBLE_OPTIONS = 3;

export function tagColorClass(name: string) {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash + name.charCodeAt(index) * (index + 1)) % TAG_PALETTE.length;
  }
  return TAG_PALETTE[hash];
}

function normalizeTagInput(value: string) {
  return value.trim().replace(/[,，]/g, "");
}

type MenuPosition = {
  top: number;
  left: number;
  width: number;
};

export function TodoTagDraftList({
  items,
  suggestions = [],
  onChange,
}: {
  items: string[];
  suggestions?: string[];
  onChange: (items: string[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const [activeIndex, setActiveIndex] = useState(-1);
  const [historyVersion, setHistoryVersion] = useState(0);
  const [suppressMenu, setSuppressMenu] = useState(false);
  const [menuPosition, setMenuPosition] = useState<MenuPosition | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const allSuggestions = useMemo(
    () => mergeTagSuggestions(getTagHistory(), suggestions),
    [suggestions, historyVersion]
  );

  const draftValue = normalizeTagInput(draft);

  const filteredOptions = useMemo(() => {
    const query = draftValue.toLocaleLowerCase();
    if (!query) return [];

    const existing = new Set(items.map((item) => item.toLocaleLowerCase()));
    return allSuggestions
      .filter((item) => !existing.has(item.toLocaleLowerCase()))
      .filter((item) => item.toLocaleLowerCase().includes(query))
      .slice(0, MAX_VISIBLE_OPTIONS);
  }, [allSuggestions, draftValue, items]);

  const showMenu = !suppressMenu && draftValue.length > 0 && filteredOptions.length > 0;

  const updateMenuPosition = () => {
    const input = inputRef.current;
    if (!input) return;

    const rect = input.getBoundingClientRect();
    setMenuPosition({
      top: rect.bottom + 4,
      left: rect.left,
      width: rect.width,
    });
  };

  useLayoutEffect(() => {
    if (!showMenu) {
      setMenuPosition(null);
      return;
    }

    updateMenuPosition();
    window.addEventListener("resize", updateMenuPosition);
    window.addEventListener("scroll", updateMenuPosition, true);
    return () => {
      window.removeEventListener("resize", updateMenuPosition);
      window.removeEventListener("scroll", updateMenuPosition, true);
    };
  }, [showMenu, draft, filteredOptions.length]);

  useEffect(() => {
    if (!showMenu) return;

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (inputRef.current?.contains(target)) return;
      if (menuRef.current?.contains(target)) return;
      setSuppressMenu(true);
      setActiveIndex(-1);
    };

    document.addEventListener("pointerdown", handlePointerDown);
    return () => document.removeEventListener("pointerdown", handlePointerDown);
  }, [showMenu]);

  const addItem = (raw: string) => {
    const next = normalizeTagInput(raw);
    if (!next) return;
    if (items.length >= 10) return;
    if (items.some((item) => item.toLocaleLowerCase() === next.toLocaleLowerCase())) {
      setDraft("");
      setSuppressMenu(false);
      setActiveIndex(-1);
      return;
    }

    recordTag(next);
    setHistoryVersion((value) => value + 1);
    onChange([...items, next]);
    setDraft("");
    setSuppressMenu(false);
    setActiveIndex(-1);
  };

  const selectOption = (item: string) => {
    addItem(item);
    window.requestAnimationFrame(() => inputRef.current?.focus());
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (filteredOptions.length === 0) return;
      setSuppressMenu(false);
      setActiveIndex((current) => (current + 1) % filteredOptions.length);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (filteredOptions.length === 0) return;
      setSuppressMenu(false);
      setActiveIndex((current) =>
        current <= 0 ? filteredOptions.length - 1 : current - 1
      );
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      setSuppressMenu(true);
      setActiveIndex(-1);
      return;
    }

    if (event.key === "Enter" || event.key === "," || event.key === "，") {
      event.preventDefault();
      if (showMenu && activeIndex >= 0 && filteredOptions[activeIndex]) {
        selectOption(filteredOptions[activeIndex]);
        return;
      }
      addItem(draft);
    }
  };

  return (
    <div className="space-y-2">
      <p className="text-[12px] font-semibold text-muted-foreground">标签</p>
      {items.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {items.map((item) => (
            <span
              key={item}
              className={cn(
                "inline-flex items-center gap-1 rounded-md px-2 py-1 text-[12px] font-medium ring-1 ring-inset",
                tagColorClass(item)
              )}
            >
              <Tag className="h-3 w-3 opacity-70" />
              {item}
              <button
                type="button"
                className="rounded-sm p-0.5 opacity-70 transition-opacity hover:opacity-100"
                aria-label={`删除标签 ${item}`}
                onClick={() => onChange(items.filter((tag) => tag !== item))}
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          ))}
        </div>
      )}

      <input
        ref={inputRef}
        value={draft}
        maxLength={32}
        placeholder="搜索或输入新标签，回车添加"
        className="h-9 w-full rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[13px] outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/45 focus:ring-2 focus:ring-primary/20"
        onChange={(event) => {
          setDraft(event.target.value);
          setSuppressMenu(false);
          setActiveIndex(0);
        }}
        onKeyDown={handleKeyDown}
      />

      {showMenu &&
        menuPosition &&
        createPortal(
          <div
            ref={menuRef}
            className="pointer-events-auto fixed z-[130] overflow-hidden rounded-lg border border-border/80 bg-popover/95 p-1 text-popover-foreground shadow-xl shadow-emerald-950/10 backdrop-blur-xl"
            style={{
              top: menuPosition.top,
              left: menuPosition.left,
              width: menuPosition.width,
            }}
          >
            {filteredOptions.map((item, index) => (
              <button
                key={item}
                type="button"
                className={cn(
                  "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2.5 text-left text-[13px] transition-colors",
                  index === activeIndex
                    ? "bg-primary/10 font-medium text-primary"
                    : "text-foreground hover:bg-foreground/6"
                )}
                onPointerDown={(event) => {
                  event.preventDefault();
                  selectOption(item);
                }}
                onMouseEnter={() => setActiveIndex(index)}
              >
                <Tag className="h-3.5 w-3.5 shrink-0 opacity-60" />
                <span className="truncate">{item}</span>
              </button>
            ))}
          </div>,
          document.body
        )}
    </div>
  );
}

export function TodoTagList({
  tags,
  compact = false,
  interactive = false,
  activeTag,
  onTagClick,
}: {
  tags: string[];
  compact?: boolean;
  interactive?: boolean;
  activeTag?: string | null;
  onTagClick?: (tag: string) => void;
}) {
  if (tags.length === 0) return null;

  return (
    <div className={cn("flex flex-wrap gap-1.5", compact ? "gap-1" : "gap-1.5")}>
      {tags.map((tag) => {
        const isActive = activeTag?.toLocaleLowerCase() === tag.toLocaleLowerCase();
        const className = cn(
          "inline-flex items-center gap-1 rounded-md font-medium ring-1 ring-inset transition-colors",
          compact ? "px-1.5 py-0.5 text-[10px]" : "px-2 py-1 text-[11px]",
          tagColorClass(tag),
          interactive && "cursor-pointer hover:brightness-95",
          isActive && "ring-2 ring-primary/40"
        );

        if (interactive && onTagClick) {
          return (
            <button
              key={tag}
              type="button"
              className={className}
              onClick={(event) => {
                event.stopPropagation();
                onTagClick(tag);
              }}
            >
              <Tag className={cn(compact ? "h-2.5 w-2.5" : "h-3 w-3", "opacity-70")} />
              {tag}
            </button>
          );
        }

        return (
          <span key={tag} className={className}>
            <Tag className={cn(compact ? "h-2.5 w-2.5" : "h-3 w-3", "opacity-70")} />
            {tag}
          </span>
        );
      })}
    </div>
  );
}
