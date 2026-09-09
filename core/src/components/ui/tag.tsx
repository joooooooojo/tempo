import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Tag as TagIcon, X } from "lucide-react";
import { getTagHistory, mergeTagSuggestions, recordTag } from "@/lib/tagHistory";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

const TAG_PALETTE = [
  "bg-sky-500/12 text-sky-700 ring-sky-500/20 dark:text-sky-300",
  "bg-violet-500/12 text-violet-700 ring-violet-500/20 dark:text-violet-300",
  "bg-amber-500/12 text-amber-700 ring-amber-500/20 dark:text-amber-300",
  "bg-rose-500/12 text-rose-700 ring-rose-500/20 dark:text-rose-300",
  "bg-teal-500/12 text-teal-700 ring-teal-500/20 dark:text-teal-300",
  "bg-orange-500/12 text-orange-700 ring-orange-500/20 dark:text-orange-300",
] as const;

const TAG_SIZE_CLASSES = {
  sm: "px-1.5 py-0.5 text-[10px]",
  default: "px-2 py-1 text-[11px]",
  lg: "px-2 py-1 text-[12px]",
} as const;

const TAG_ICON_SIZE_CLASSES = {
  sm: "size-2.5",
  default: "size-3",
  lg: "size-3",
} as const;

const MAX_VISIBLE_OPTIONS = 3;

export type TagSize = keyof typeof TAG_SIZE_CLASSES;

export function tagColorClass(name: string) {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash + name.charCodeAt(index) * (index + 1)) % TAG_PALETTE.length;
  }
  return TAG_PALETTE[hash];
}

export type TagProps = {
  value: string;
  size?: TagSize;
  active?: boolean;
  className?: string;
  trailing?: ReactNode;
  onClick?: (value: string, event: MouseEvent<HTMLButtonElement>) => void;
};

export function Tag({
  value,
  size = "default",
  active = false,
  className,
  trailing,
  onClick,
}: TagProps) {
  const classes = cn(
    "inline-flex items-center gap-1 rounded-md font-medium ring-1 ring-inset transition-colors",
    TAG_SIZE_CLASSES[size],
    tagColorClass(value),
    onClick && "cursor-pointer hover:brightness-95",
    active && "ring-2 ring-primary/40",
    className
  );
  const content = (
    <>
      <TagIcon className={cn(TAG_ICON_SIZE_CLASSES[size], "shrink-0 opacity-70")} />
      {value}
      {trailing}
    </>
  );

  if (onClick) {
    return (
      <button type="button" className={classes} onClick={(event) => onClick(value, event)}>
        {content}
      </button>
    );
  }

  return <span className={classes}>{content}</span>;
}

export function TagList({
  items,
  size = "default",
  interactive = false,
  activeItem,
  onItemClick,
}: {
  items: string[];
  size?: TagSize;
  interactive?: boolean;
  activeItem?: string | null;
  onItemClick?: (item: string) => void;
}) {
  if (items.length === 0) return null;

  return (
    <div className={cn("flex flex-wrap", size === "sm" ? "gap-1" : "gap-1.5")}>
      {items.map((item) => (
        <Tag
          key={item}
          value={item}
          size={size}
          active={activeItem?.toLocaleLowerCase() === item.toLocaleLowerCase()}
          onClick={
            interactive && onItemClick
              ? (value, event) => {
                  event.stopPropagation();
                  onItemClick(value);
                }
              : undefined
          }
        />
      ))}
    </div>
  );
}

function normalizeTagInput(value: string) {
  return value.trim().replace(/[,，]/g, "");
}

type MenuPosition = {
  top: number;
  left: number;
  width: number;
};

export function TagDraftList({
  items,
  suggestions = [],
  label = "标签",
  inputName = "tag-draft",
  placeholder = "搜索或输入新标签，回车添加",
  maxItems = 10,
  maxLength = 32,
  onChange,
}: {
  items: string[];
  suggestions?: string[];
  label?: string;
  inputName?: string;
  placeholder?: string;
  maxItems?: number;
  maxLength?: number;
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
    if (!next || items.length >= maxItems) return;
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
    <div className="flex flex-col gap-2">
      <Label htmlFor={inputName} className="text-[12px] font-semibold text-muted-foreground">
        {label}
      </Label>
      {items.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {items.map((item) => (
            <Tag
              key={item}
              value={item}
              size="lg"
              trailing={
                <button
                  type="button"
                  className="rounded-sm p-0.5 opacity-70 transition-opacity hover:opacity-100"
                  aria-label={`删除标签 ${item}`}
                  onClick={() => onChange(items.filter((tag) => tag !== item))}
                >
                  <X className="size-3" />
                </button>
              }
            />
          ))}
        </div>
      )}

      <Input
        ref={inputRef}
        id={inputName}
        value={draft}
        maxLength={maxLength}
        name={inputName}
        autoComplete="off"
        placeholder={placeholder}
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
                <TagIcon className="size-3.5 shrink-0 opacity-60" />
                <span className="truncate">{item}</span>
              </button>
            ))}
          </div>,
          document.body
        )}
    </div>
  );
}
