import { Copy, MoreVertical, Pencil, Pin, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  TableCell,
  TableRow,
} from "@/components/ui/table";
import { cn, formatRelativeTime, previewLines } from "@/lib/utils";
import { TextWithLinks } from "@/components/TextWithLinks";
import type { Snippet } from "@/types";

export function SnippetRow({
  snippet,
  actionMenuOpen,
  onActionMenuOpenChange,
  onOpenDetail,
  onUse,
  onEdit,
  onTogglePinned,
  onDelete,
}: {
  snippet: Snippet;
  actionMenuOpen: boolean;
  onActionMenuOpenChange: (open: boolean) => void;
  onOpenDetail: () => void;
  onUse: () => void;
  onEdit: () => void;
  onTogglePinned: () => void;
  onDelete: () => void;
}) {
  return (
    <TableRow
      role="button"
      tabIndex={0}
      className={cn(
        "h-[58px] cursor-pointer border-b border-border/45 text-[12px] transition-colors last:border-b-0 hover:bg-foreground/[0.025]",
        snippet.pinned && "bg-primary/[0.035]"
      )}
      onClick={onOpenDetail}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpenDetail();
        }
      }}
    >
      <TableCell className="px-3 py-2 align-middle">
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex min-w-0 items-center gap-1.5">
            {snippet.pinned && <Pin className="h-3 w-3 shrink-0 text-primary" />}
            <span className="truncate font-medium text-foreground" title={snippet.title}>
              {snippet.title}
            </span>
          </div>
          {snippet.shortcut && (
            <span className="w-fit rounded-md bg-foreground/6 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
              {snippet.shortcut}
            </span>
          )}
        </div>
      </TableCell>
      <TableCell className="max-w-0 px-3 py-2 align-middle">
        <div className="m-0 block max-w-full truncate text-[12px] leading-[17px] text-foreground/88">
          <TextWithLinks text={previewLines(snippet.content, 1)} />
        </div>
      </TableCell>
      <TableCell className="px-3 py-2 align-middle">
        <span className="truncate text-muted-foreground">
          {snippet.group_name || "未分组"}
        </span>
      </TableCell>
      <TableCell className="px-3 py-2 align-middle">
        <div className="flex min-w-0 flex-wrap gap-1">
          {snippet.tags.length === 0 ? (
            <span className="text-muted-foreground">-</span>
          ) : (
            snippet.tags.slice(0, 3).map((tag) => (
              <span
                key={tag}
                className="max-w-[72px] truncate rounded-md bg-foreground/6 px-1.5 py-0.5 text-[11px] text-muted-foreground"
                title={tag}
              >
                {tag}
              </span>
            ))
          )}
        </div>
      </TableCell>
      <TableCell className="px-3 py-2 align-middle">
        <div className="flex flex-col gap-1 text-muted-foreground">
          <span>{snippet.use_count} 次</span>
          <span className="text-[11px]">
            {snippet.last_used_at ? formatRelativeTime(snippet.last_used_at) : "未使用"}
          </span>
        </div>
      </TableCell>
      <TableCell
        className="px-2 py-2 align-middle"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <div className="flex gap-1">
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-primary"
            title="使用"
            aria-label="使用短语"
            onClick={onUse}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <SnippetRowActionMenu
            open={actionMenuOpen}
            onOpenChange={onActionMenuOpenChange}
            pinned={snippet.pinned}
            onTogglePinned={onTogglePinned}
            onEdit={onEdit}
            onDelete={onDelete}
          />
        </div>
      </TableCell>
    </TableRow>
  );
}

export function SnippetRowActionMenu({
  open,
  onOpenChange,
  pinned,
  onTogglePinned,
  onEdit,
  onDelete,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  pinned: boolean;
  onTogglePinned: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const runAction = (action: () => void) => {
    onOpenChange(false);
    action();
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8 text-muted-foreground hover:bg-foreground/6 hover:text-foreground"
          aria-label="更多操作"
          title="更多操作"
        >
          <MoreVertical className="h-4 w-4 shrink-0" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" side="bottom" className="w-36 p-1">
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
          onClick={() => runAction(onTogglePinned)}
        >
          <Pin className={cn("h-3.5 w-3.5 text-muted-foreground", pinned && "fill-current text-primary")} />
          {pinned ? "取消置顶" : "置顶"}
        </button>
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-popover-foreground transition-colors hover:bg-foreground/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25"
          onClick={() => runAction(onEdit)}
        >
          <Pencil className="h-3.5 w-3.5 text-muted-foreground" />
          编辑
        </button>
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-rose-600 transition-colors hover:bg-rose-500/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-rose-500/25 dark:text-rose-300"
          onClick={() => runAction(onDelete)}
        >
          <Trash2 className="h-3.5 w-3.5" />
          删除
        </button>
      </PopoverContent>
    </Popover>
  );
}

