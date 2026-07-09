import { useMemo, useState } from "react";
import { Plus, Tag, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const TAG_PALETTE = [
  "bg-sky-500/12 text-sky-700 ring-sky-500/20 dark:text-sky-300",
  "bg-violet-500/12 text-violet-700 ring-violet-500/20 dark:text-violet-300",
  "bg-amber-500/12 text-amber-700 ring-amber-500/20 dark:text-amber-300",
  "bg-rose-500/12 text-rose-700 ring-rose-500/20 dark:text-rose-300",
  "bg-teal-500/12 text-teal-700 ring-teal-500/20 dark:text-teal-300",
  "bg-orange-500/12 text-orange-700 ring-orange-500/20 dark:text-orange-300",
] as const;

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

  const visibleSuggestions = useMemo(() => {
    const query = normalizeTagInput(draft).toLocaleLowerCase();
    if (!query) return [];
    const existing = new Set(items.map((item) => item.toLocaleLowerCase()));
    return suggestions
      .filter((item) => !existing.has(item.toLocaleLowerCase()))
      .filter((item) => item.toLocaleLowerCase().includes(query))
      .slice(0, 6);
  }, [draft, items, suggestions]);

  const addItem = (raw: string) => {
    const next = normalizeTagInput(raw);
    if (!next) return;
    if (items.length >= 10) return;
    if (items.some((item) => item.toLocaleLowerCase() === next.toLocaleLowerCase())) {
      setDraft("");
      return;
    }
    onChange([...items, next]);
    setDraft("");
  };

  return (
    <div className="space-y-2">
      <p className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        标签
      </p>
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
      <div className="space-y-1.5">
        <div className="flex gap-2">
          <input
            value={draft}
            maxLength={32}
            placeholder="输入标签，回车添加"
            className="h-9 min-w-0 flex-1 rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[13px] outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/45 focus:ring-2 focus:ring-primary/20"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === "," || event.key === "，") {
                event.preventDefault();
                addItem(draft);
              }
            }}
          />
          <Button
            type="button"
            variant="outline"
            className="h-9 shrink-0 px-3"
            disabled={items.length >= 10}
            onClick={() => addItem(draft)}
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
        {visibleSuggestions.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {visibleSuggestions.map((item) => (
              <button
                key={item}
                type="button"
                className={cn(
                  "rounded-md px-2 py-1 text-[11px] font-medium ring-1 ring-inset transition-opacity hover:opacity-80",
                  tagColorClass(item)
                )}
                onClick={() => addItem(item)}
              >
                {item}
              </button>
            ))}
          </div>
        )}
      </div>
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
