import { useEffect, useRef, useState, type ClipboardEvent, type PointerEvent, type ReactNode } from "react";
import {
  ClipboardList,
  MessageSquarePlus,
  MoreVertical,
  Pencil,
  Pin,
  Timer,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn, formatDurationShort } from "@/lib/utils";
import type { TodoFocusSummary, TodoImage, TodoNote, TodoNoteImage } from "@/types";
import {
  escapeRegExp,
  formatTodoDate,
  normalizeSearch,
  type DraftImage,
} from "./todoPageUtils";

export interface NoteDraft {
  body: string;
  images: DraftImage[];
  open?: boolean;
  saving?: boolean;
}

export function TodoExpandableSection({
  open,
  children,
}: {
  open: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "grid transition-[grid-template-rows] duration-300 ease-in-out motion-reduce:transition-none",
        open ? "grid-rows-[1fr]" : "grid-rows-[0fr]"
      )}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="overflow-hidden">
        <div className="p-1">
          <div
            className={cn(
              "mt-2 pl-12 transition-opacity duration-300 ease-in-out motion-reduce:transition-none",
              open ? "opacity-100" : "opacity-0"
            )}
          >
            {children}
          </div>
        </div>
      </div>
    </div>
  );
}

export function TodoStat({
  label,
  value,
  tone = "default",
}: {
  label: string;
  value: number;
  tone?: "default" | "warning";
}) {
  return (
    <Card>
      <CardContent className="p-3.5">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </p>
        <p className={cn("stat-value mt-1 text-2xl font-bold", tone === "warning" ? "text-amber-500" : "text-primary")}>
          {value}
        </p>
      </CardContent>
    </Card>
  );
}

export function TodoEmptyState({ text }: { text: string }) {
  return (
    <div className="flex flex-col items-center py-14 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-lg bg-foreground/5">
        <ClipboardList className="h-5 w-5 text-muted-foreground" />
      </div>
      <p className="mt-3 text-sm font-medium">{text}</p>
    </div>
  );
}

export function ImagePreviewViewport({ src, alt }: { src: string; alt: string }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(1);
  const panRef = useRef({ x: 0, y: 0 });
  const dragRef = useRef<{
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    zoomRef.current = 1;
    panRef.current = { x: 0, y: 0 };
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setDragging(false);
    dragRef.current = null;
  }, [src]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();

      const rect = viewport.getBoundingClientRect();
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const centerX = rect.width / 2;
      const centerY = rect.height / 2;

      const direction = event.deltaY < 0 ? 1 : -1;
      const currentZoom = zoomRef.current;
      const currentPan = panRef.current;
      const nextZoom = Math.min(
        5,
        Math.max(0.5, Number((currentZoom + direction * 0.15).toFixed(2))),
      );
      if (nextZoom === currentZoom) return;

      const ratio = nextZoom / currentZoom;
      const nextPan = {
        x: currentPan.x * ratio + (mouseX - centerX) * (1 - ratio),
        y: currentPan.y * ratio + (mouseY - centerY) * (1 - ratio),
      };

      zoomRef.current = nextZoom;
      panRef.current = nextPan;
      setZoom(nextZoom);
      setPan(nextPan);
    };

    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [src]);

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragging(true);
    dragRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      originX: panRef.current.x,
      originY: panRef.current.y,
    };
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;

    const nextPan = {
      x: drag.originX + event.clientX - drag.startX,
      y: drag.originY + event.clientY - drag.startY,
    };
    panRef.current = nextPan;
    setPan(nextPan);
  };

  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div
      ref={viewportRef}
      className={cn(
        "flex min-h-0 flex-1 select-none overflow-hidden rounded-lg bg-foreground/[0.04] touch-none",
        dragging ? "cursor-grabbing" : "cursor-grab",
      )}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      <div className="flex h-full w-full items-center justify-center">
        <img
          src={src}
          alt={alt}
          draggable={false}
          className="max-h-full max-w-full origin-center object-contain will-change-transform"
          style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
        />
      </div>
    </div>
  );
}

export function HighlightText({ value, query }: { value: string; query: string }) {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) return <>{value}</>;

  const matcher = new RegExp(`(${escapeRegExp(trimmedQuery)})`, "gi");
  const parts = value.split(matcher);

  return (
    <>
      {parts.map((part, index) =>
        normalizeSearch(part) === normalizeSearch(trimmedQuery) ? (
          <mark
            key={`${part}-${index}`}
            className="rounded-[3px] bg-amber-200/75 px-0.5 text-amber-950 dark:bg-amber-300/25 dark:text-amber-100"
          >
            {part}
          </mark>
        ) : (
          <span key={`${part}-${index}`}>{part}</span>
        )
      )}
    </>
  );
}

export function TodoFocusStats({ summary }: { summary: TodoFocusSummary }) {
  return (
    <div className="mb-4 rounded-lg border border-emerald-500/15 bg-emerald-500/8 px-4 py-3">
      <p className="text-[10px] font-semibold uppercase tracking-wider text-emerald-700/80 dark:text-emerald-300/80">
        专注记录
      </p>
      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-emerald-950/75 dark:text-emerald-50/75">
        <span>今日 {summary.sessions_today} 轮 · {formatDurationShort(summary.total_seconds_today)}</span>
        <span>累计 {summary.sessions_all} 轮 · {formatDurationShort(summary.total_seconds_all)}</span>
        {summary.last_focused_at && (
          <span>上次专注 {formatTodoDate(summary.last_focused_at)}</span>
        )}
      </div>
    </div>
  );
}

export function TodoRowActionMenu({
  open,
  onOpenChange,
  showStartFocus,
  onStartFocus,
  pinned,
  onTogglePinned,
  onAddNote,
  onEdit,
  onDelete,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  showStartFocus?: boolean;
  onStartFocus?: () => void;
  pinned: boolean;
  onTogglePinned: () => void;
  onAddNote: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className={cn(
            "h-8 w-8 shrink-0 p-0 text-muted-foreground transition-opacity hover:bg-foreground/6 hover:text-foreground",
            open ? "opacity-100" : "opacity-100 sm:opacity-0 sm:group-hover/todo:opacity-100 sm:group-focus-within/todo:opacity-100"
          )}
          aria-label="更多操作"
        >
          <MoreVertical className="h-4 w-4 shrink-0" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" side="bottom" className="w-36 p-1">
        {showStartFocus && onStartFocus && (
          <button
            type="button"
            className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
            onClick={onStartFocus}
          >
            <Timer className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-300" />
            开始专注
          </button>
        )}
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
          onClick={onTogglePinned}
        >
          <Pin className={cn("h-3.5 w-3.5 text-muted-foreground", pinned && "fill-current text-primary")} />
          {pinned ? "取消置顶" : "置顶"}
        </button>
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
          onClick={onAddNote}
        >
          <MessageSquarePlus className="h-3.5 w-3.5 text-muted-foreground" />
          追加备注
        </button>
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
          onClick={onEdit}
        >
          <Pencil className="h-3.5 w-3.5 text-muted-foreground" />
          编辑
        </button>
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-rose-600 transition-colors hover:bg-rose-500/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-rose-500/25 dark:text-rose-300"
          onClick={onDelete}
        >
          <Trash2 className="h-3.5 w-3.5" />
          删除
        </button>
      </PopoverContent>
    </Popover>
  );
}

export function TodoImages({
  images,
  onDelete,
  onPreview,
}: {
  images: TodoImage[];
  onDelete?: (image: TodoImage) => void;
  onPreview?: (image: TodoImage) => void;
}) {
  if (images.length === 0) return null;

  return (
    <div className="mt-3 flex flex-wrap gap-2">
      {images.map((image) => (
        <span key={image.id} className="group relative block h-20 w-24 overflow-hidden rounded-lg border border-border/60 bg-foreground/5">
          <button
            type="button"
            className="block h-full w-full overflow-hidden text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
            aria-label="预览图片"
            onClick={() => onPreview?.(image)}
          >
            <img src={image.data_url} alt="" className="h-full w-full object-cover" draggable={false} />
          </button>
          {onDelete && (
            <button
              type="button"
              className="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md bg-background/85 text-muted-foreground opacity-0 shadow-sm transition-opacity hover:text-rose-600 group-hover:opacity-100"
              aria-label="删除图片"
              onClick={(event) => {
                event.stopPropagation();
                onDelete(image);
              }}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

export function TodoNotes({
  notes,
  searchQuery,
  onDelete,
  onPreview,
}: {
  notes: TodoNote[];
  searchQuery?: string;
  onDelete?: (note: TodoNote) => void;
  onPreview: (image: TodoNoteImage) => void;
}) {
  if (notes.length === 0) return null;

  return (
    <div className="mt-3 space-y-2 border-l border-primary/25 pl-3">
      {notes.map((note) => (
        <div
          key={note.id}
          className="group/note rounded-md px-2.5 py-2 transition-colors hover:bg-foreground/[0.025] focus-within:bg-foreground/[0.025]"
        >
          <div className="flex items-center justify-between gap-3">
            <span className="inline-flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
              <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary/70" />
              <span>追加备注于</span>
              <span className="font-normal opacity-80">{formatTodoDate(note.created_at)}</span>
            </span>
            {onDelete && (
              <button
                type="button"
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground/65 opacity-0 transition-[background,color,opacity] hover:bg-rose-500/10 hover:text-rose-600 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-rose-500/25 group-hover/note:opacity-100 group-focus-within/note:opacity-100 dark:hover:text-rose-300"
                aria-label="删除备注"
                onClick={() => onDelete(note)}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
          {note.body && (
            <p className="mt-1.5 whitespace-pre-wrap break-words text-[13px] leading-5 text-foreground/78">
              <HighlightText value={note.body} query={searchQuery ?? ""} />
            </p>
          )}
          <NoteImages images={note.images} onPreview={onPreview} />
        </div>
      ))}
    </div>
  );
}

export function NoteImages({
  images,
  onPreview,
}: {
  images: TodoNoteImage[];
  onPreview: (image: TodoNoteImage) => void;
}) {
  if (images.length === 0) return null;

  return (
    <div className="mt-2 flex flex-wrap gap-2">
      {images.map((image) => (
        <button
          key={image.id}
          type="button"
          className="block h-16 w-20 overflow-hidden rounded-lg border border-border/60 bg-foreground/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          aria-label="预览备注图片"
          onClick={() => onPreview(image)}
        >
          <img src={image.data_url} alt="" className="h-full w-full object-cover" draggable={false} />
        </button>
      ))}
    </div>
  );
}

export function NoteComposer({
  draft,
  onBodyChange,
  onPaste,
  onDeleteImage,
  onCancel,
  onSubmit,
}: {
  draft: NoteDraft;
  onBodyChange: (body: string) => void;
  onPaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
  onDeleteImage: (image: DraftImage) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const canSubmit = Boolean(draft.body.trim() || draft.images.length > 0);

  if (!draft.open) {
    return null;
  }

  return (
    <div className="mt-3 space-y-2">
      <textarea
        value={draft.body}
        maxLength={1000}
        placeholder="追加备注"
        className="block min-h-16 w-full resize-none rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 py-2.5 text-[13px] leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/30"
        onChange={(event) => onBodyChange(event.target.value)}
        onPaste={onPaste}
      />
      {draft.images.length > 0 && (
        <div>
          <ImageStrip images={draft.images} onDelete={onDeleteImage} />
        </div>
      )}
      <div className="flex items-center justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 min-w-16 text-muted-foreground"
          disabled={draft.saving}
          onClick={onCancel}
        >
          取消
        </Button>
        <Button
          type="button"
          size="sm"
          className="h-8 min-w-20"
          disabled={!canSubmit || draft.saving}
          onClick={onSubmit}
        >
          追加
        </Button>
      </div>
    </div>
  );
}

export function ImageStrip({
  images,
  onDelete,
}: {
  images: DraftImage[];
  onDelete: (image: DraftImage) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {images.map((image) => (
        <span key={image.local_id} className="group relative block h-20 w-24 overflow-hidden rounded-lg border border-border/60 bg-foreground/5">
          <img src={image.data_url} alt="" className="h-full w-full object-cover" draggable={false} />
          <button
            type="button"
            className="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md bg-background/85 text-muted-foreground shadow-sm hover:text-rose-600"
            aria-label="删除图片"
            onClick={() => onDelete(image)}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </span>
      ))}
    </div>
  );
}
