import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useLocation, useNavigate } from "react-router-dom";
import {
  Copy,
  Folder,
  FolderPlus,
  Loader2,
  MoreVertical,
  Pencil,
  Pin,
  Search,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DataTable } from "@/components/ui/data-table";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import { cn, formatRelativeTime, previewLines } from "@/lib/utils";
import { CodeEditor } from "@/components/CodeEditor";
import { CodeHighlight, SNIPPET_LANGUAGE_OPTIONS } from "@/components/CodeHighlight";
import { TextWithLinks } from "@/components/TextWithLinks";
import type { Snippet, SnippetGroup } from "@/types";

type GroupFilter = "all" | "ungrouped" | `${number}`;
type SortMode = "smart" | "used" | "updated" | "title";

type EditorState = {
  id?: number;
  title: string;
  content: string;
  tags: string;
  groupId: string;
  shortcut: string;
  language: string;
};

const emptyEditor: EditorState = {
  title: "",
  content: "",
  tags: "",
  groupId: "none",
  shortcut: "",
  language: "plain",
};

const SORT_OPTIONS: Array<{ value: SortMode; label: string }> = [
  { value: "smart", label: "智能排序" },
  { value: "used", label: "使用最多" },
  { value: "updated", label: "最近更新" },
  { value: "title", label: "按标题" },
];

export function SnippetsPage() {
  const location = useLocation();
  const navigate = useNavigate();
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [groups, setGroups] = useState<SnippetGroup[]>([]);
  const [query, setQuery] = useState("");
  const [groupFilter, setGroupFilter] = useState<GroupFilter>("all");
  const [sort, setSort] = useState<SortMode>("smart");
  const [loading, setLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [detailSnippet, setDetailSnippet] = useState<Snippet | null>(null);
  const [moreSettingsOpen, setMoreSettingsOpen] = useState(false);
  const [editor, setEditor] = useState<EditorState>(emptyEditor);
  const [saving, setSaving] = useState(false);
  const [groupDialogOpen, setGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState("");
  const [creatingGroup, setCreatingGroup] = useState(false);
  const [actionMenuId, setActionMenuId] = useState<number | null>(null);

  const groupId = useMemo(() => groupFilterToId(groupFilter), [groupFilter]);
  const pinnedCount = snippets.filter((snippet) => snippet.pinned).length;
  const usedCount = snippets.reduce((total, snippet) => total + snippet.use_count, 0);

  const load = useCallback(
    async (showLoading = false) => {
      if (showLoading) setLoading(true);
      try {
        const [nextGroups, nextSnippets] = await Promise.all([
          api.getSnippetGroups(),
          api.getSnippets(query || undefined, groupId, sort),
        ]);
        setGroups(nextGroups);
        setSnippets(nextSnippets);
        setDetailSnippet((current) => {
          if (!current) return null;
          return nextSnippets.find((item) => item.id === current.id) ?? current;
        });
      } catch (error) {
        toast.error(error instanceof Error ? error.message : "加载短语失败");
      } finally {
        if (showLoading) setLoading(false);
      }
    },
    [groupId, query, sort]
  );

  useEffect(() => {
    const timer = window.setTimeout(() => void load(true), 160);
    return () => window.clearTimeout(timer);
  }, [load]);

  useEffect(() => {
    const unlisten = listen("snippets-update", () => void load(false));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  const openCreate = useCallback(() => {
    setEditor({
      ...emptyEditor,
      groupId: "none",
    });
    setMoreSettingsOpen(false);
    setEditorOpen(true);
  }, []);

  useEffect(() => {
    const state = location.state as { createSnippet?: boolean } | null;
    if (!state?.createSnippet) return;
    openCreate();
    navigate(location.pathname, { replace: true, state: null });
  }, [location.pathname, location.state, navigate, openCreate]);

  const openEdit = (snippet: Snippet) => {
    setEditor({
      id: snippet.id,
      title: snippet.title,
      content: snippet.content,
      tags: snippet.tags.join(", "),
      groupId: snippet.group_id ? String(snippet.group_id) : "none",
      shortcut: snippet.shortcut ?? "",
      language: snippet.language || "plain",
    });
    setMoreSettingsOpen(false);
    setEditorOpen(true);
  };

  const saveEditor = async () => {
    const title = editor.title.trim();
    const content = editor.content.trim();
    const tags = splitTags(editor.tags);
    const nextGroupId = editor.groupId === "none" ? null : Number(editor.groupId);
    const shortcut = editor.shortcut.trim() || null;
    const language = editor.language === "plain" ? null : editor.language;

    if (!title || !content) {
      toast.error("请填写标题和内容");
      return;
    }

    setSaving(true);
    try {
      if (editor.id) {
        await api.updateSnippet(editor.id, title, content, tags, nextGroupId, shortcut, language);
      } else {
        await api.createSnippet(title, content, tags, nextGroupId, shortcut, language);
      }
      setEditorOpen(false);
      toast.success("已保存");
      void load(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "保存失败");
    } finally {
      setSaving(false);
    }
  };

  const createGroup = async () => {
    const name = newGroupName.trim();
    if (!name) {
      toast.error("请输入分组名称");
      return;
    }
    setCreatingGroup(true);
    try {
      await api.createSnippetGroup(name);
      setNewGroupName("");
      toast.success("分组已创建");
      void load(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "创建分组失败");
    } finally {
      setCreatingGroup(false);
    }
  };

  const deleteGroup = async (group: SnippetGroup) => {
    if (!confirm(`删除「${group.name}」分组？分组内短语会保留为未分组。`)) return;
    try {
      await api.deleteSnippetGroup(group.id);
      if (groupFilter === String(group.id)) setGroupFilter("all");
      toast.success("分组已删除");
      void load(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "删除分组失败");
    }
  };

  const useSnippet = async (snippet: Snippet) => {
    try {
      const updated = await api.copySnippetToClipboard(snippet.id);
      setSnippets((current) =>
        current.map((item) => (item.id === updated.id ? updated : item))
      );
      toast.success("已使用短语");
      void load(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "复制失败");
    }
  };

  const togglePinned = async (snippet: Snippet) => {
    const nextPinned = !snippet.pinned;
    setSnippets((current) =>
      current.map((item) => (item.id === snippet.id ? { ...item, pinned: nextPinned } : item))
    );
    try {
      await api.pinSnippet(snippet.id, nextPinned);
      void load(false);
    } catch (error) {
      setSnippets((current) =>
        current.map((item) => (item.id === snippet.id ? { ...item, pinned: snippet.pinned } : item))
      );
      toast.error(error instanceof Error ? error.message : "操作失败");
    }
  };

  const deleteSnippet = async (snippet: Snippet) => {
    if (!confirm(`删除「${snippet.title}」？`)) return;
    setSnippets((current) => current.filter((item) => item.id !== snippet.id));
    try {
      await api.deleteSnippet(snippet.id);
      toast.success("已删除");
      void load(false);
    } catch (error) {
      setSnippets((current) => [snippet, ...current]);
      toast.error(error instanceof Error ? error.message : "删除失败");
    }
  };

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-6xl flex-1 flex-col gap-3">
      <div className="grid shrink-0 grid-cols-1 gap-2 md:grid-cols-3">
        <Metric label="短语" value={snippets.length} />
        <Metric label="固定" value={pinnedCount} />
        <Metric label="累计使用" value={usedCount} />
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索"
            className="h-9 border-0 pl-9 glass-subtle"
          />
        </div>

        <Select
          items={groupOptions(groups)}
          value={groupFilter}
          onValueChange={(value) => value && setGroupFilter(value as GroupFilter)}
        >
          <SelectTrigger className="h-9 w-37.5 border-0 glass-subtle">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              <SelectItem value="all">全部分组</SelectItem>
              <SelectItem value="ungrouped">未分组</SelectItem>
              {groups.map((group) => (
                <SelectItem key={group.id} value={String(group.id)}>
                  {group.name}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>

        <Select
          items={SORT_OPTIONS}
          value={sort}
          onValueChange={(value) => value && setSort(value as SortMode)}
        >
          <SelectTrigger className="h-9 w-32 border-0 glass-subtle">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {SORT_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>

        <Button
          variant="outline"
          className="h-9 border-0 glass-subtle"
          onClick={() => setGroupDialogOpen(true)}
        >
          <FolderPlus className="h-4 w-4" />
          分组
        </Button>
        <Button size="lg" className="px-4" onClick={openCreate}>
          新建
        </Button>
      </div>

      <DataTable
        loading={loading}
        loadingContent={
          <>
            <Loader2 className="h-4 w-4 animate-spin" />
            加载中...
          </>
        }
        empty={snippets.length === 0}
        emptyContent={query ? "没有匹配的短语" : "还没有快捷短语，先新建一个常用回复吧"}
        scrollAreaLabel="快捷短语列表"
      >
        <Table className="w-full table-fixed border-collapse text-left">
          <TableHeader className="sticky top-0 z-10 bg-background/90 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground backdrop-blur supports-[backdrop-filter]:bg-background/75">
            <TableRow className="border-b border-border/55 hover:bg-transparent">
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">标题</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">内容</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">分组</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">标签</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-3 py-2 text-muted-foreground">使用</TableHead>
              <TableHead className="h-auto whitespace-nowrap px-2 py-2 text-muted-foreground">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {snippets.map((snippet) => (
              <SnippetRow
                key={snippet.id}
                snippet={snippet}
                actionMenuOpen={actionMenuId === snippet.id}
                onActionMenuOpenChange={(open) => setActionMenuId(open ? snippet.id : null)}
                onOpenDetail={() => setDetailSnippet(snippet)}
                onUse={() => void useSnippet(snippet)}
                onTogglePinned={() => {
                  setActionMenuId(null);
                  void togglePinned(snippet);
                }}
                onEdit={() => {
                  setActionMenuId(null);
                  openEdit(snippet);
                }}
                onDelete={() => {
                  setActionMenuId(null);
                  void deleteSnippet(snippet);
                }}
              />
            ))}
          </TableBody>
        </Table>
      </DataTable>

      <Dialog
        open={Boolean(detailSnippet)}
        onOpenChange={(open) => {
          if (!open) setDetailSnippet(null);
        }}
      >
        <DialogContent
          className="todo-create-dialog !flex max-h-[min(720px,calc(100vh-2rem))] w-[calc(100vw-2rem)] max-w-[720px] flex-col gap-0 overflow-hidden rounded-xl border-border/80 p-0 sm:max-w-[720px]"
        >
          {detailSnippet && (
            <>
              <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-4 pr-12">
                <div className="flex min-w-0 items-start gap-3 pr-2">
                  <div className="min-w-0 flex-1">
                    <DialogTitle className="truncate text-[18px] font-bold">
                      {detailSnippet.title}
                    </DialogTitle>
                    <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{detailSnippet.group_name || "未分组"}</span>
                      {detailSnippet.shortcut && (
                        <span className="rounded-md bg-foreground/6 px-1.5 py-0.5 font-mono text-[11px]">
                          {detailSnippet.shortcut}
                        </span>
                      )}
                      {detailSnippet.language && detailSnippet.language !== "plain" && (
                        <span className="rounded-md bg-primary/10 px-1.5 py-0.5 font-medium text-primary">
                          {SNIPPET_LANGUAGE_OPTIONS.find(
                            (option) => option.value === detailSnippet.language
                          )?.label || detailSnippet.language}
                        </span>
                      )}
                      <span>使用 {detailSnippet.use_count} 次</span>
                      {detailSnippet.pinned && (
                        <span className="inline-flex items-center gap-1 text-primary">
                          <Pin className="h-3 w-3 fill-current" />
                          已置顶
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              </DialogHeader>

              <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden px-6 py-5">
                {detailSnippet.tags.length > 0 && (
                  <div className="flex flex-wrap gap-1.5">
                    {detailSnippet.tags.map((tag) => (
                      <span
                        key={tag}
                        className="rounded-md bg-foreground/6 px-1.5 py-0.5 text-[11px] text-muted-foreground"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                )}
                <ScrollArea
                  className="h-80 shrink-0 overflow-hidden rounded-md border border-border/50 bg-muted/40"
                  scrollbars="both"
                  viewportClassName="no-scrollbar"
                  aria-label="短语内容"
                >
                  {detailSnippet.language && detailSnippet.language !== "plain" ? (
                    <CodeHighlight
                      code={detailSnippet.content}
                      language={detailSnippet.language}
                      overflow={false}
                      className="m-0 border-0 bg-transparent"
                    />
                  ) : (
                    <div className="whitespace-pre-wrap break-words p-3 text-[13px] leading-6 text-foreground/90">
                      <TextWithLinks text={detailSnippet.content} />
                    </div>
                  )}
                </ScrollArea>
              </div>

              <DialogFooter className="mx-0 mb-0 flex w-full shrink-0 flex-row items-center gap-3 rounded-none border-t border-border/60 bg-foreground/[0.018] px-6 py-4 sm:space-x-0">
                <Button
                  className="h-9"
                  variant="outline"
                  onClick={() => setDetailSnippet(null)}
                >
                  关闭
                </Button>
                <div className="ml-auto flex items-center gap-2">
                  <Button
                    className="h-9"
                    variant="outline"
                    onClick={() => {
                      const snippet = detailSnippet;
                      setDetailSnippet(null);
                      openEdit(snippet);
                    }}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                    编辑
                  </Button>
                  <Button
                    className="h-9"
                    onClick={() => void useSnippet(detailSnippet)}
                  >
                    <Copy className="h-3.5 w-3.5" />
                    使用
                  </Button>
                </div>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog
        open={editorOpen}
        onOpenChange={(open) => {
          setEditorOpen(open);
          if (!open) setMoreSettingsOpen(false);
        }}
        modal={moreSettingsOpen ? "trap-focus" : true}
      >
        <DialogContent className="flex flex-col !h-[80vh] !max-h-[calc(100vh-2rem)] !w-[80vw] !max-w-[calc(100vw-2rem)] gap-0 overflow-hidden p-0">
          <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-4 pr-12">
            <DialogTitle>{editor.id ? "编辑短语" : "新建短语"}</DialogTitle>
          </DialogHeader>
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden px-6 py-4">
            <div className="grid shrink-0 gap-3 md:grid-cols-[minmax(0,1fr)_220px]">
              <div className="flex flex-col gap-2">
                <Label htmlFor="snippet-title">标题</Label>
                <Input
                  id="snippet-title"
                  value={editor.title}
                  onChange={(event) =>
                    setEditor((state) => ({ ...state, title: event.target.value }))
                  }
                  placeholder="例如：售后开场白"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label>代码语言</Label>
                <Select
                  items={SNIPPET_LANGUAGE_OPTIONS.map((option) => ({
                    value: option.value,
                    label: option.label,
                  }))}
                  value={editor.language}
                  onValueChange={(value) =>
                    value && setEditor((state) => ({ ...state, language: value }))
                  }
                >
                  <SelectTrigger className="h-9 w-full bg-transparent shadow-none">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent
                    overlayLayer
                    searchable
                    searchPlaceholder="搜索语言..."
                  >
                    <SelectGroup>
                      {SNIPPET_LANGUAGE_OPTIONS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="flex min-h-0 flex-1 flex-col gap-2">
              <Label htmlFor="snippet-content">内容</Label>
              <CodeEditor
                id="snippet-content"
                value={editor.content}
                language={editor.language}
                placeholder="输入要快速复用的完整文字..."
                onChange={(content) => setEditor((state) => ({ ...state, content }))}
              />
            </div>
          </div>
          <DialogFooter className="mx-0 mb-0 flex w-full shrink-0 flex-row items-center gap-3 rounded-none border-t border-border/60 bg-foreground/[0.018] px-6 py-4 sm:space-x-0">
            <SnippetMoreSettings
              open={moreSettingsOpen}
              onOpenChange={setMoreSettingsOpen}
              groups={groups}
              groupId={editor.groupId}
              tags={editor.tags}
              shortcut={editor.shortcut}
              onGroupIdChange={(groupId) => setEditor((state) => ({ ...state, groupId }))}
              onTagsChange={(tags) => setEditor((state) => ({ ...state, tags }))}
              onShortcutChange={(shortcut) => setEditor((state) => ({ ...state, shortcut }))}
            />
            <div className="ml-auto flex items-center gap-2">
              <Button className="h-9" variant="outline" onClick={() => setEditorOpen(false)}>
                取消
              </Button>
              <Button className="h-9" disabled={saving} onClick={() => void saveEditor()}>
                保存
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={groupDialogOpen} onOpenChange={setGroupDialogOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>短语分组</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <div className="flex gap-2">
              <Input
                value={newGroupName}
                onChange={(event) => setNewGroupName(event.target.value)}
                placeholder="新分组名称"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void createGroup();
                  }
                }}
              />
              <Button className="h-9" disabled={creatingGroup} onClick={() => void createGroup()}>
                添加
              </Button>
            </div>
            <div className="flex max-h-[280px] flex-col gap-1 overflow-y-auto">
              {groups.length === 0 ? (
                <p className="rounded-lg border border-dashed px-3 py-8 text-center text-[13px] text-muted-foreground">
                  暂无分组
                </p>
              ) : (
                groups.map((group) => (
                  <div
                    key={group.id}
                    className="flex items-center justify-between gap-2 rounded-lg border border-border/60 px-3 py-2"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <Folder className="h-4 w-4 text-muted-foreground" />
                      <span className="truncate text-[13px] font-medium">{group.name}</span>
                    </div>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8 text-destructive"
                      title="删除分组"
                      onClick={() => void deleteGroup(group)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))
              )}
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function SnippetRow({
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

function SnippetRowActionMenu({
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

function SnippetMoreSettings({
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
        <DialogContent
          showOverlay={false}
          className="todo-create-dialog max-h-[min(520px,85vh)] max-w-[440px] gap-0 overflow-hidden rounded-xl border-border/80 p-0"
          onOpenAutoFocus={(event) => event.preventDefault()}
        >
          <DialogHeader className="border-b border-border/60 px-5 py-4 pr-12">
            <DialogTitle className="text-[18px] font-bold">更多配置</DialogTitle>
          </DialogHeader>

          <div className="no-scrollbar max-h-[min(380px,60vh)] space-y-5 overflow-y-auto px-5 py-5">
            <div className="space-y-2">
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

            <div className="space-y-2">
              <Label htmlFor="snippet-more-shortcut">快捷词</Label>
              <Input
                id="snippet-more-shortcut"
                value={shortcut}
                onChange={(event) => onShortcutChange(event.target.value)}
                placeholder="/hello"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="snippet-more-tags">标签</Label>
              <Input
                id="snippet-more-tags"
                value={tags}
                onChange={(event) => onTagsChange(event.target.value)}
                placeholder="逗号分隔，例如：客服, 售后"
              />
            </div>
          </div>

          <DialogFooter className="mx-0 mb-0 rounded-none border-t border-border/60 bg-foreground/[0.018] px-5 py-3 sm:justify-end">
            <Button type="button" className="h-9 min-w-20" onClick={() => onOpenChange(false)}>
              完成
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg border border-border/55 bg-foreground/[0.018] px-3 py-2">
      <p className="text-[11px] font-medium text-muted-foreground">{label}</p>
      <p className="mt-0.5 text-lg font-semibold tabular-nums text-foreground">{value}</p>
    </div>
  );
}

function splitTags(value: string) {
  return value
    .split(/[,，]/)
    .map((tag) => tag.trim())
    .filter(Boolean);
}

function groupFilterToId(value: GroupFilter) {
  if (value === "all") return undefined;
  if (value === "ungrouped") return 0;
  return Number(value);
}

function groupOptions(groups: SnippetGroup[]) {
  return [
    { value: "all", label: "全部分组" },
    { value: "ungrouped", label: "未分组" },
    ...groups.map((group) => ({ value: String(group.id), label: group.name })),
  ];
}
