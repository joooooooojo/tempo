import { useState } from "react";
import { Check, Circle, Plus, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { TodoSubtask } from "@/types";

export function TodoSubtaskDraftList({
  items,
  onChange,
}: {
  items: string[];
  onChange: (items: string[]) => void;
}) {
  const [draft, setDraft] = useState("");

  const addItem = () => {
    const next = draft.trim();
    if (!next) return;
    onChange([...items, next]);
    setDraft("");
  };

  return (
    <div className="space-y-2">
      <p className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        子任务
      </p>
      {items.length > 0 && (
        <div className="space-y-1.5">
          {items.map((item, index) => (
            <div
              key={`${item}-${index}`}
              className="flex items-center gap-2 rounded-lg border border-border/60 bg-foreground/[0.02] px-2.5 py-2"
            >
              <Circle className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70" />
              <span className="min-w-0 flex-1 truncate text-[13px]">{item}</span>
              <button
                type="button"
                className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-foreground"
                aria-label="删除子任务"
                onClick={() => onChange(items.filter((_, itemIndex) => itemIndex !== index))}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="flex gap-2">
        <input
          value={draft}
          maxLength={120}
          placeholder="添加子任务"
          className="h-9 min-w-0 flex-1 rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[13px] outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/45 focus:ring-2 focus:ring-primary/20"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              addItem();
            }
          }}
        />
        <Button type="button" variant="outline" className="h-9 shrink-0 px-3" onClick={addItem}>
          <Plus className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

export function TodoSubtaskList({
  subtasks,
  editable = false,
  readOnly = false,
  compact = false,
  onToggle,
  onDelete,
  onAdd,
}: {
  subtasks: TodoSubtask[];
  editable?: boolean;
  readOnly?: boolean;
  compact?: boolean;
  onToggle?: (subtask: TodoSubtask, completed: boolean) => void;
  onDelete?: (subtask: TodoSubtask) => void;
  onAdd?: (title: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const interactive = !readOnly && Boolean(onToggle);
  const canEdit = !readOnly && editable;

  if (subtasks.length === 0 && !canEdit) return null;

  const submitDraft = () => {
    const next = draft.trim();
    if (!next || !onAdd) return;
    onAdd(next);
    setDraft("");
  };

  return (
    <div className={cn(compact ? "mt-2 space-y-1.5" : "mt-4 space-y-2")}>
      {!compact && (
        <p className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          子任务
        </p>
      )}
      {subtasks.map((subtask) => (
        <div
          key={subtask.id}
          className={cn(
            "flex items-center gap-2 rounded-lg border border-border/60 bg-foreground/[0.02]",
            compact ? "px-2 py-1.5" : "px-2.5 py-2"
          )}
        >
          {interactive ? (
            <button
              type="button"
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-primary"
              aria-label={subtask.completed ? "恢复子任务" : "完成子任务"}
              onClick={() => onToggle?.(subtask, !subtask.completed)}
            >
              {subtask.completed ? (
                <Check className="h-3.5 w-3.5 text-primary" />
              ) : (
                <Circle className="h-3.5 w-3.5" />
              )}
            </button>
          ) : (
            <span className="flex h-6 w-6 shrink-0 items-center justify-center text-muted-foreground">
              {subtask.completed ? (
                <Check className="h-3.5 w-3.5 text-primary" />
              ) : (
                <Circle className="h-3.5 w-3.5" />
              )}
            </span>
          )}
          <span
            className={cn(
              "min-w-0 flex-1 text-left text-[13px]",
              subtask.completed && "text-muted-foreground line-through"
            )}
          >
            {subtask.title}
          </span>
          {canEdit && onDelete && (
            <button
              type="button"
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-rose-500/10 hover:text-rose-600"
              aria-label="删除子任务"
              onClick={() => onDelete(subtask)}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      ))}
      {canEdit && onAdd && (
        <div className="flex gap-2">
          <input
            value={draft}
            maxLength={120}
            placeholder="添加子任务"
            className="h-8 min-w-0 flex-1 rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[12px] outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/45 focus:ring-2 focus:ring-primary/20"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                submitDraft();
              }
            }}
          />
          <Button type="button" variant="outline" size="sm" className="h-8 px-2.5" onClick={submitDraft}>
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}
    </div>
  );
}
