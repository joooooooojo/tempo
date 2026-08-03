import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Copy,
  Folder,
  FolderPlus,
  Loader2,
  Pencil,
  Pin,
  Search,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import type { BuiltinAppProps } from "@/apps/types";
import { Button } from "@/components/ui/button";
import { DataTable } from "@/components/ui/data-table";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { SAVE_SHORTCUT_LABEL, useSaveShortcut } from "@/hooks/useSaveShortcut";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import { CodeEditor } from "@/components/CodeEditor";
import { CodeHighlight, SNIPPET_LANGUAGE_OPTIONS } from "@/components/CodeHighlight";
import { TextWithLinks } from "@/components/TextWithLinks";
import type { Snippet, SnippetGroup } from "@/types";
import { SnippetRow } from "@/builtin-plugins/snippets/pages/SnippetRow";
import {
  SnippetMoreSettings,
  groupFilterToId,
  groupOptions,
  splitTags,
  type GroupFilter,
} from "@/builtin-plugins/snippets/pages/SnippetMoreSettings";

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


export function SnippetsPage({ openCreateOnMount }: BuiltinAppProps) {
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
    if (!openCreateOnMount) return;
    openCreate();
  }, [openCreate, openCreateOnMount]);

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
    if (saving) return;
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

  useSaveShortcut(() => void saveEditor(), {
    active: editorOpen,
    enabled: !saving,
  });

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
        <DialogPanel className="todo-create-dialog max-h-[min(720px,calc(100vh-2rem))] w-[calc(100vw-2rem)] max-w-[720px] sm:max-w-[720px]">
          {detailSnippet && (
            <>
              <DialogHeader>
                <DialogTitle className="min-w-0 text-[18px] font-bold">
                  <span className="truncate">{detailSnippet.title}</span>
                </DialogTitle>
                <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
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
                      <Pin className="size-3 fill-current" />
                      已置顶
                    </span>
                  )}
                </div>
              </DialogHeader>

              <DialogContent scrollable={false} className="flex flex-col gap-4 overflow-hidden">
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
              </DialogContent>

              <DialogFooter className="sm:justify-between">
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
                    <Pencil className="size-3.5" />
                    编辑
                  </Button>
                  <Button
                    className="h-9"
                    onClick={() => void useSnippet(detailSnippet)}
                  >
                    <Copy className="size-3.5" />
                    使用
                  </Button>
                </div>
              </DialogFooter>
            </>
          )}
        </DialogPanel>
      </Dialog>

      <Dialog
        open={editorOpen}
        onOpenChange={(open) => {
          setEditorOpen(open);
          if (!open) setMoreSettingsOpen(false);
        }}
        modal={moreSettingsOpen ? "trap-focus" : true}
      >
        <DialogPanel className="!h-[80vh] !max-h-[calc(100vh-2rem)] !w-[80vw] !max-w-[calc(100vw-2rem)]">
          <DialogHeader>
            <DialogTitle>{editor.id ? "编辑短语" : "新建短语"}</DialogTitle>
          </DialogHeader>
          <DialogContent scrollable={false} className="flex flex-col gap-4 overflow-hidden py-4">
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
          </DialogContent>
          <DialogFooter className="sm:justify-between">
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
              <Button
                className="h-9"
                disabled={saving}
                onClick={() => void saveEditor()}
                title={`保存（${SAVE_SHORTCUT_LABEL}）`}
              >
                保存
              </Button>
            </div>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog open={groupDialogOpen} onOpenChange={setGroupDialogOpen}>
        <DialogPanel className="max-w-md">
          <DialogHeader>
            <DialogTitle>短语分组</DialogTitle>
          </DialogHeader>
          <DialogContent className="flex flex-col gap-3">
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
                      <Folder className="size-4 text-muted-foreground" />
                      <span className="truncate text-[13px] font-medium">{group.name}</span>
                    </div>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="size-8 text-destructive"
                      title="删除分组"
                      onClick={() => void deleteGroup(group)}
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                ))
              )}
            </div>
          </DialogContent>
        </DialogPanel>
      </Dialog>
    </div>
  );
}

