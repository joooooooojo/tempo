import { SlidersHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { SnippetGroup } from "@/types";

export function SnippetMoreSettings({
  open,
  onOpenChange,
  groups,
  groupId,
  tags,
  shortcut,
  onGroupIdChange,
  onTagsChange,
  onShortcutChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  groups: SnippetGroup[];
  groupId: string;
  tags: string;
  shortcut: string;
  onGroupIdChange: (value: string) => void;
  onTagsChange: (value: string) => void;
  onShortcutChange: (value: string) => void;
}) {
  const hasActive =
    groupId !== "none" || Boolean(tags.trim()) || Boolean(shortcut.trim());

  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className={cn(
          "relative h-9 gap-1.5 px-3 text-muted-foreground hover:text-foreground",
          hasActive && "text-foreground"
        )}
        onClick={() => onOpenChange(true)}
      >
        <SlidersHorizontal className="h-4 w-4 shrink-0" />
        <span>更多配置</span>
        {hasActive && (
          <span
            className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-primary"
            aria-hidden
          />
        )}
      </Button>

      <Dialog open={open} onOpenChange={onOpenChange} modal="trap-focus">
        <DialogPanel
          showOverlay={false}
          className="todo-create-dialog max-h-[min(520px,85vh)] max-w-[440px]"
          onOpenAutoFocus={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>更多配置</DialogTitle>
          </DialogHeader>

          <DialogContent className="flex flex-col gap-5">
            <div className="flex flex-col gap-2">
              <Label>分组</Label>
              <Select
                items={[
                  { value: "none", label: "未分组" },
                  ...groups.map((group) => ({ value: String(group.id), label: group.name })),
                ]}
                value={groupId}
                onValueChange={(value) => value && onGroupIdChange(value)}
              >
                <SelectTrigger className="h-9 w-full bg-transparent shadow-none">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent overlayLayer>
                  <SelectGroup>
                    <SelectItem value="none">未分组</SelectItem>
                    {groups.map((group) => (
                      <SelectItem key={group.id} value={String(group.id)}>
                        {group.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="snippet-more-shortcut">快捷词</Label>
              <Input
                id="snippet-more-shortcut"
                value={shortcut}
                onChange={(event) => onShortcutChange(event.target.value)}
                placeholder="/hello"
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="snippet-more-tags">标签</Label>
              <Input
                id="snippet-more-tags"
                value={tags}
                onChange={(event) => onTagsChange(event.target.value)}
                placeholder="逗号分隔，例如：客服, 售后"
              />
            </div>
          </DialogContent>

          <DialogFooter>
            <Button type="button" className="h-9 min-w-20" onClick={() => onOpenChange(false)}>
              完成
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </>
  );
}

export function splitTags(value: string) {
  return value
    .split(/[,，]/)
    .map((tag) => tag.trim())
    .filter(Boolean);
}


export type GroupFilter = "all" | "ungrouped" | `${number}`;

export function groupFilterToId(value: GroupFilter) {
  if (value === "all") return undefined;
  if (value === "ungrouped") return 0;
  return Number(value);
}

export function groupOptions(groups: SnippetGroup[]) {
  return [
    { value: "all", label: "全部分组" },
    { value: "ungrouped", label: "未分组" },
    ...groups.map((group) => ({ value: String(group.id), label: group.name })),
  ];
}
