import {
  useEffect,
  useMemo,
  useState,
  type ClipboardEvent,
  type FormEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  CheckCircle2,
  Circle,
  ClipboardList,
  Download,
  ImagePlus,
  MessageSquarePlus,
  MoreVertical,
  Pencil,
  Pin,
  Search,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { MarkdownPreview } from "@/components/todos/MarkdownPreview";
import { TodoCreateDialog, type DraftTodoImage } from "@/components/todos/TodoCreateDialog";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { api, type TodoImageInput } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { TodoImage, TodoItem, TodoNote, TodoNoteImage } from "@/types";

type TodoFilter = "active" | "completed";

type DraftImage = DraftTodoImage;

interface PreviewImage {
  data_url: string;
  label: string;
}

interface NoteDraft {
  body: string;
  images: DraftImage[];
  open?: boolean;
  saving?: boolean;
}

const filters: Array<{ value: TodoFilter; label: string }> = [
  { value: "active", label: "未完成" },
  { value: "completed", label: "已完成" },
];

const MAX_IMAGES_PER_NOTE = 4;
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export function TodoPage() {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [noteDrafts, setNoteDrafts] = useState<Record<number, NoteDraft>>({});
  const [createOpen, setCreateOpen] = useState(false);
  const [detailId, setDetailId] = useState<number | null>(null);
  const [previewImage, setPreviewImage] = useState<PreviewImage | null>(null);
  const [filter, setFilter] = useState<TodoFilter>("active");
  const [searchQuery, setSearchQuery] = useState("");
  const [actionMenuId, setActionMenuId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editContent, setEditContent] = useState("");
  const [editDueAt, setEditDueAt] = useState("");

  useEffect(() => {
    let cancelled = false;

    api.getTodos()
      .then((items) => {
        if (!cancelled) setTodos(sortTodos(items));
      })
      .catch((error) => {
        console.error(error);
        toast.error("待办加载失败");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<TodoItem>("todo-created", (event) => {
      setTodos((current) => sortTodos([event.payload, ...current.filter((todo) => todo.id !== event.payload.id)]));
      setFilter("active");
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const activeCount = todos.filter((todo) => !todo.completed).length;
  const completedCount = todos.length - activeCount;
  const dueSoonCount = todos.filter((todo) => !todo.completed && isDueSoon(todo.due_at)).length;
  const completionRate = todos.length === 0 ? 0 : Math.round((completedCount / todos.length) * 100);

  const visibleTodos = useMemo(() => {
    const filtered = filter === "active"
      ? todos.filter((todo) => !todo.completed)
      : todos.filter((todo) => todo.completed);
    const query = normalizeSearch(searchQuery);
    if (!query) return filtered;
    return filtered.filter((todo) => matchesTodoSearch(todo, query));
  }, [filter, searchQuery, todos]);
  const editingTodo = useMemo(
    () => (editingId === null ? null : todos.find((todo) => todo.id === editingId) ?? null),
    [editingId, todos]
  );
  const detailTodo = useMemo(
    () => (detailId === null ? null : todos.find((todo) => todo.id === detailId) ?? null),
    [detailId, todos]
  );

  const resetDraft = () => {
    setTitle("");
    setContent("");
    setDueAt("");
  };

  const handleCreateOpenChange = (nextOpen: boolean) => {
    if (saving) return;

    setCreateOpen(nextOpen);
    if (!nextOpen) resetDraft();
  };

  const updateNoteDraft = (todoId: number, update: (draft: NoteDraft) => NoteDraft) => {
    setNoteDrafts((current) => ({
      ...current,
      [todoId]: update(current[todoId] ?? { body: "", images: [], open: true }),
    }));
  };

  const resetNoteDraft = (todoId: number) => {
    setNoteDrafts((current) => {
      const next = { ...current };
      delete next[todoId];
      return next;
    });
  };

  const exportTodosBackup = async () => {
    if (exporting) return;

    const path = await save({
      title: "导出待办备份",
      defaultPath: `screen-time-todos-${new Date().toISOString().slice(0, 10)}.zip`,
      filters: [{ name: "待办备份", extensions: ["zip"] }],
    });
    if (!path) return;

    setExporting(true);
    try {
      await api.exportTodosBackup(path);
      toast.success("待办备份已导出");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setExporting(false);
    }
  };

  const importTodosBackup = async () => {
    if (importing) return;

    const path = await open({
      title: "导入待办备份",
      multiple: false,
      filters: [{ name: "待办备份", extensions: ["zip", "json"] }],
    });
    if (!path || Array.isArray(path)) return;

    setImporting(true);
    try {
      const items = await api.importTodosBackup(path);
      setTodos(sortTodos(items));
      setFilter("active");
      toast.success("待办备份已导入");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setImporting(false);
    }
  };

  const handleNotePaste = async (todoId: number, event: ClipboardEvent<HTMLElement>) => {
    const hasPastedImage = Array.from(event.clipboardData.items).some(
      (item) => item.kind === "file" && item.type.startsWith("image/")
    );
    if (!hasPastedImage) return;

    event.preventDefault();
    event.stopPropagation();

    const pastedImages = await imagesFromClipboard(event);
    if (pastedImages.length === 0) return;

    const current = noteDrafts[todoId] ?? { body: "", images: [] };
    const available = MAX_IMAGES_PER_NOTE - current.images.length;
    if (available <= 0) {
      toast.error(`每条备注最多添加 ${MAX_IMAGES_PER_NOTE} 张图片`);
      return;
    }

    updateNoteDraft(todoId, (draft) => ({
      ...draft,
      images: [...draft.images, ...pastedImages.slice(0, available)],
    }));
    toast.success(pastedImages.length > available ? "已添加部分图片" : "已粘贴图片");
  };

  const submitNote = async (todo: TodoItem) => {
    const draft = noteDrafts[todo.id] ?? { body: "", images: [] };
    if (!draft.body.trim() && draft.images.length === 0) {
      toast.error("请输入备注内容");
      return;
    }

    updateNoteDraft(todo.id, (current) => ({ ...current, saving: true }));
    try {
      const updated = await api.addTodoNote(
        todo.id,
        draft.body,
        draft.images.map(toTodoImageInput)
      );
      setTodos((current) => replaceTodo(current, updated));
      resetNoteDraft(todo.id);
      toast.success("备注已追加");
    } catch (error) {
      toast.error(errorMessage(error));
      updateNoteDraft(todo.id, (current) => ({ ...current, saving: false }));
    }
  };

  const deleteNote = async (note: TodoNote) => {
    try {
      const updated = await api.deleteTodoNote(note.id);
      setTodos((current) => replaceTodo(current, updated));
      toast.success("备注已删除", {
        action: {
          label: "撤销",
          onClick: async () => {
            try {
              const restored = await api.restoreTodoNote(note);
              setTodos((current) => replaceTodo(current, restored));
              toast.success("备注已恢复");
            } catch (error) {
              toast.error(errorMessage(error));
            }
          },
        },
      });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const handleAdd = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!title.trim()) {
      toast.error("请输入标题");
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(
        title,
        content,
        toIsoDateTime(dueAt)
      );
      setTodos((current) => sortTodos([created, ...current]));
      resetDraft();
      setCreateOpen(false);
      setFilter("active");
      toast.success("已添加");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const toggleTodo = async (todo: TodoItem) => {
    try {
      const updated = await api.setTodoCompleted(todo.id, !todo.completed);
      setTodos((current) => replaceTodo(current, updated));
      toast.success(updated.completed ? "已完成" : "已恢复", {
        action: {
          label: "撤销",
          onClick: async () => {
            try {
              const restored = await api.setTodoCompleted(todo.id, todo.completed);
              setTodos((current) => replaceTodo(current, restored));
              toast.success(todo.completed ? "已恢复完成状态" : "已撤销完成");
            } catch (error) {
              toast.error(errorMessage(error));
            }
          },
        },
      });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const toggleTodoPinned = async (todo: TodoItem) => {
    const nextPinned = !todo.pinned_at;

    try {
      const updated = await api.setTodoPinned(todo.id, nextPinned);
      setTodos((current) => replaceTodo(current, updated));
      toast.success(nextPinned ? "已置顶" : "已取消置顶", {
        action: {
          label: "撤销",
          onClick: async () => {
            try {
              const restored = await api.setTodoPinned(todo.id, Boolean(todo.pinned_at));
              setTodos((current) => replaceTodo(current, restored));
              toast.success(todo.pinned_at ? "已恢复置顶" : "已撤销置顶");
            } catch (error) {
              toast.error(errorMessage(error));
            }
          },
        },
      });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const startEdit = (todo: TodoItem) => {
    setEditingId(todo.id);
    setEditTitle(todo.title);
    setEditContent(todo.content);
    setEditDueAt(toDateTimeLocalValue(todo.due_at));
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditTitle("");
    setEditContent("");
    setEditDueAt("");
  };

  const handleEditOpenChange = (nextOpen: boolean) => {
    if (saving) return;
    if (!nextOpen) cancelEdit();
  };

  const commitEdit = async (todo: TodoItem) => {
    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      toast.error("请输入标题");
      return;
    }

    setSaving(true);
    try {
      const updated = await api.updateTodoDetails(todo.id, nextTitle, editContent, toIsoDateTime(editDueAt));
      setTodos((current) => replaceTodo(current, updated));
      cancelEdit();
      toast.success("已更新");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const deleteTodo = async (todo: TodoItem) => {
    try {
      await api.deleteTodo(todo.id);
      setTodos((current) => current.filter((item) => item.id !== todo.id));
      if (detailId === todo.id) setDetailId(null);
      if (editingId === todo.id) cancelEdit();
      toast.success("已删除", {
        action: {
          label: "撤销",
          onClick: async () => {
            try {
              const restored = await api.restoreTodo(todo);
              setTodos((current) => upsertTodo(current, restored));
              toast.success("待办已恢复");
            } catch (error) {
              toast.error(errorMessage(error));
            }
          },
        },
      });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const deleteImage = async (image: TodoImage) => {
    try {
      const updated = await api.deleteTodoImage(image.id);
      setTodos((current) => replaceTodo(current, updated));
      toast.success("图片已删除");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div className="grid grid-cols-4 gap-3">
        <TodoStat label="未完成" value={activeCount} />
        <TodoStat label="即将截止" value={dueSoonCount} tone={dueSoonCount > 0 ? "warning" : "default"} />
        <TodoStat label="已完成" value={completedCount} />
        <Card className="overflow-hidden">
          <CardContent className="p-3.5">
            <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              完成率
            </p>
            <div className="mt-2 flex items-center gap-3">
              <p className="stat-value w-12 text-xl font-bold text-primary">{completionRate}%</p>
              <div className="progress-track h-1.5 min-w-0 flex-1 rounded-sm bg-foreground/8">
                <div
                  className="progress-fill h-full rounded-sm bg-gradient-to-r from-emerald-300 to-teal-400 transition-all duration-500"
                  style={{ width: `${completionRate}%` }}
                />
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      <TodoCreateDialog
        open={createOpen}
        todoTitle={title}
        todoContent={content}
        dueAt={dueAt}
        saving={saving}
        onOpenChange={handleCreateOpenChange}
        onTitleChange={setTitle}
        onContentChange={setContent}
        onDueAtChange={setDueAt}
        onSubmit={handleAdd}
      />

      <TodoCreateDialog
        open={Boolean(editingTodo)}
        heading="编辑待办事项"
        todoTitle={editTitle}
        todoContent={editContent}
        dueAt={editDueAt}
        saving={saving}
        submitLabel="保存"
        bodyExtra={
          editingTodo?.images.length ? (
            <TodoImages
              images={editingTodo.images}
              onDelete={deleteImage}
              onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: editingTodo.title })}
            />
          ) : undefined
        }
        onOpenChange={handleEditOpenChange}
        onTitleChange={setEditTitle}
        onContentChange={setEditContent}
        onDueAtChange={setEditDueAt}
        onSubmit={(event) => {
          event.preventDefault();
          if (editingTodo) void commitEdit(editingTodo);
        }}
      />

      <Dialog
        open={Boolean(detailTodo)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDetailId(null);
        }}
      >
        <DialogContent className="todo-create-dialog max-w-[680px] gap-0 overflow-hidden rounded-xl border-border/80 p-0">
          {detailTodo && (
            <>
              <DialogHeader className="border-b border-border/60 px-5 py-4 pr-12">
                <DialogTitle className="truncate text-[18px] font-bold">
                  <HighlightText value={detailTodo.title} query={searchQuery} />
                </DialogTitle>
              </DialogHeader>
              <div className="max-h-[68vh] overflow-y-auto px-5 py-4">
                <div className="mb-4 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                  <span>
                    {detailTodo.completed && detailTodo.completed_at
                      ? `完成于 ${formatTodoDate(detailTodo.completed_at)}`
                      : `创建于 ${formatTodoDate(detailTodo.created_at)}`}
                  </span>
                  {detailTodo.due_at && (
                    <span className={cn("rounded-md px-1.5 py-0.5 font-medium", dueBadgeClass(detailTodo))}>
                      截止 {formatTodoDate(detailTodo.due_at)}
                    </span>
                  )}
                </div>

                <MarkdownPreview value={detailTodo.content} />

                <TodoImages
                  images={detailTodo.images}
                  onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: detailTodo.title })}
                />
                <TodoNotes
                  notes={detailTodo.notes}
                  searchQuery={searchQuery}
                  onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: detailTodo.title })}
                />
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(previewImage)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setPreviewImage(null);
        }}
      >
        <DialogContent className="max-w-[760px] gap-3 p-3">
          <DialogHeader className="px-1 pr-8">
            <DialogTitle className="truncate text-[15px]">图片预览</DialogTitle>
          </DialogHeader>
          {previewImage && (
            <div className="flex max-h-[72vh] min-h-0 items-center justify-center overflow-hidden rounded-lg bg-foreground/[0.04]">
              <img
                src={previewImage.data_url}
                alt={previewImage.label}
                className="max-h-[72vh] w-full object-contain"
                draggable={false}
              />
            </div>
          )}
        </DialogContent>
      </Dialog>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="inline-flex h-9 items-center gap-1 rounded-lg glass-subtle p-1">
          {filters.map((item) => (
            <button
              key={item.value}
              type="button"
              className={cn(
                "inline-flex h-7 min-w-20 items-center justify-center rounded-md px-3 text-[13px] font-medium text-muted-foreground transition-[background,box-shadow,color]",
                filter === item.value
                  ? "bg-white/70 text-foreground shadow-sm ring-1 ring-white/70 dark:bg-white/10 dark:ring-white/10"
                  : "hover:text-foreground"
              )}
              onClick={() => setFilter(item.value)}
            >
              {item.label}
              <span className="ml-1.5 text-[11px] opacity-70">{filterCount(item.value, todos)}</span>
            </button>
          ))}
        </div>

        <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
          <label className="relative min-w-[180px] max-w-[280px] flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <input
              value={searchQuery}
              placeholder="搜索待办、正文、备注"
              className="h-9 w-full rounded-lg border border-border/70 bg-white/55 pl-8 pr-8 text-[13px] text-foreground shadow-sm shadow-emerald-950/[0.03] outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/45 focus:ring-2 focus:ring-primary/20 dark:bg-white/[0.045]"
              onChange={(event) => setSearchQuery(event.target.value)}
            />
            {searchQuery && (
              <button
                type="button"
                className="absolute right-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-foreground"
                aria-label="清空搜索"
                onClick={() => setSearchQuery("")}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </label>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 gap-1.5 px-3"
            disabled={importing}
            onClick={() => void importTodosBackup()}
          >
            <Upload className="h-3.5 w-3.5" />
            导入
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 gap-1.5 px-3"
            disabled={exporting}
            onClick={() => void exportTodosBackup()}
          >
            <Download className="h-3.5 w-3.5" />
            备份
          </Button>
          <Button
            size="sm"
            className="h-9 px-4"
            onClick={() => setCreateOpen(true)}
          >
            新建
          </Button>
        </div>
      </div>

      <Card className="overflow-hidden">
        <CardContent className="p-0">
          {loading ? (
            <TodoEmptyState text="加载中..." />
          ) : visibleTodos.length === 0 ? (
            <TodoEmptyState text={emptyText(filter, Boolean(searchQuery.trim()))} />
          ) : (
            <div className="divide-y divide-border/45">
              {visibleTodos.map((todo) => {
                const noteDraft = noteDrafts[todo.id] ?? { body: "", images: [] };
                const summary = todoSummary(todo);

                return (
                  <div
                    key={todo.id}
                    className={cn(
                      "group/todo relative grid grid-cols-[36px_minmax(0,1fr)] items-start gap-3 px-4 py-3.5 transition-colors hover:bg-foreground/[0.03] focus-within:bg-foreground/[0.03]",
                      todo.completed && "bg-foreground/[0.018]"
                    )}
                  >
                    <button
                      type="button"
                      className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-primary"
                      aria-label={todo.completed ? "恢复待办" : "完成待办"}
                      onClick={() => void toggleTodo(todo)}
                    >
                      {todo.completed ? (
                        <CheckCircle2 className="h-5 w-5 text-primary" />
                      ) : (
                        <Circle className="h-5 w-5" />
                      )}
                    </button>

                    <div className="min-w-0 text-left">
                      <div className="flex min-w-0 items-center gap-2 pr-9">
                        {todo.pinned_at && !todo.completed && (
                          <span
                            className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary"
                            aria-label="已置顶"
                            title="已置顶"
                          >
                            <Pin className="h-3 w-3 fill-current" />
                          </span>
                        )}
                        <button
                          type="button"
                          className={cn(
                            "min-w-0 truncate text-left text-[14px] font-semibold transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
                            todo.completed && "text-muted-foreground line-through"
                          )}
                          onClick={() => setDetailId(todo.id)}
                        >
                          <HighlightText value={todo.title} query={searchQuery} />
                        </button>
                        {todo.images.length > 0 && (
                          <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-foreground/5 px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground">
                            <ImagePlus className="h-3 w-3" />
                            {todo.images.length}
                          </span>
                        )}
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-2 pr-9 text-[11px] text-muted-foreground">
                        <span>
                          {todo.completed && todo.completed_at
                            ? `完成于 ${formatTodoDate(todo.completed_at)}`
                            : `创建于 ${formatTodoDate(todo.created_at)}`}
                        </span>
                        {todo.due_at && (
                          <span className={cn("rounded-md px-1.5 py-0.5 font-medium", dueBadgeClass(todo))}>
                            截止 {formatTodoDate(todo.due_at)}
                          </span>
                        )}
                      </div>
                      {summary && (
                        <button
                          type="button"
                          className="mt-1.5 block max-w-full truncate text-left text-[12px] leading-5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                          onClick={() => setDetailId(todo.id)}
                        >
                          <HighlightText value={summary} query={searchQuery} />
                        </button>
                      )}
                      <TodoImages
                        images={todo.images}
                        onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: todo.title })}
                      />
                      <TodoNotes
                        notes={todo.notes}
                        searchQuery={searchQuery}
                        onDelete={deleteNote}
                        onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: todo.title })}
                      />
                      <NoteComposer
                        draft={noteDraft}
                        onBodyChange={(body) =>
                          updateNoteDraft(todo.id, (current) => ({ ...current, body, open: true }))
                        }
                        onPaste={(event) => void handleNotePaste(todo.id, event)}
                        onDeleteImage={(image) =>
                          updateNoteDraft(todo.id, (current) => ({
                            ...current,
                            images: current.images.filter((item) => item.local_id !== image.local_id),
                          }))
                        }
                        onCancel={() => resetNoteDraft(todo.id)}
                        onSubmit={() => void submitNote(todo)}
                      />
                    </div>

                    <TodoRowActionMenu
                      open={actionMenuId === todo.id}
                      onOpenChange={(open) => setActionMenuId(open ? todo.id : null)}
                      onAddNote={() => {
                        setActionMenuId(null);
                        updateNoteDraft(todo.id, (current) => ({ ...current, open: true }));
                      }}
                      pinned={Boolean(todo.pinned_at)}
                      onTogglePinned={() => {
                        setActionMenuId(null);
                        void toggleTodoPinned(todo);
                      }}
                      onEdit={() => {
                        setActionMenuId(null);
                        startEdit(todo);
                      }}
                      onDelete={() => {
                        setActionMenuId(null);
                        void deleteTodo(todo);
                      }}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function TodoStat({
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

function TodoEmptyState({ text }: { text: string }) {
  return (
    <div className="flex flex-col items-center py-14 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-lg bg-foreground/5">
        <ClipboardList className="h-5 w-5 text-muted-foreground" />
      </div>
      <p className="mt-3 text-sm font-medium">{text}</p>
    </div>
  );
}

function HighlightText({ value, query }: { value: string; query: string }) {
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

function TodoRowActionMenu({
  open,
  onOpenChange,
  pinned,
  onTogglePinned,
  onAddNote,
  onEdit,
  onDelete,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
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
            "absolute right-3 top-3 h-8 w-8 text-muted-foreground transition-opacity hover:bg-foreground/6 hover:text-foreground",
            open ? "opacity-100" : "opacity-100 sm:opacity-0 sm:group-hover/todo:opacity-100 sm:group-focus-within/todo:opacity-100"
          )}
          aria-label="更多操作"
        >
          <MoreVertical className="h-4 w-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" side="bottom" className="w-36 p-1">
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

function TodoImages({
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

function TodoNotes({
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

function NoteImages({
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

function NoteComposer({
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
        className="block min-h-16 w-full resize-none rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 py-2.5 text-[13px] leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
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

function ImageStrip({
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

function filterCount(filter: TodoFilter, todos: TodoItem[]) {
  if (filter === "active") return todos.filter((todo) => !todo.completed).length;
  return todos.filter((todo) => todo.completed).length;
}

function emptyText(filter: TodoFilter, searching = false) {
  if (searching) return "没有匹配的待办";
  if (filter === "completed") return "暂无已完成事项";
  return "暂无未完成事项";
}

function replaceTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos(items.map((item) => (item.id === todo.id ? todo : item)));
}

function upsertTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos([todo, ...items.filter((item) => item.id !== todo.id)]);
}

function sortTodos(items: TodoItem[]) {
  return [...items].sort((a, b) => {
    if (a.completed !== b.completed) return Number(a.completed) - Number(b.completed);

    if (!a.completed) {
      const aPinned = Boolean(a.pinned_at);
      const bPinned = Boolean(b.pinned_at);
      if (aPinned !== bPinned) return Number(bPinned) - Number(aPinned);

      if (aPinned && bPinned) {
        const aPinnedTime = toTimestamp(a.pinned_at);
        const bPinnedTime = toTimestamp(b.pinned_at);
        if (aPinnedTime !== bPinnedTime) return bPinnedTime - aPinnedTime;
      }

      const aDue = toTimestamp(a.due_at);
      const bDue = toTimestamp(b.due_at);
      if (aDue !== bDue) return aDue - bDue;
    }

    const aTime = new Date(a.completed_at ?? a.created_at).getTime();
    const bTime = new Date(b.completed_at ?? b.created_at).getTime();
    return bTime - aTime || b.id - a.id;
  });
}

function todoSummary(todo: TodoItem) {
  const cleaned = plainTextSummary(todo.content);
  if (!cleaned || cleaned === todo.title.trim()) return "";
  return cleaned;
}

function matchesTodoSearch(todo: TodoItem, query: string) {
  return [
    todo.title,
    todo.content,
    ...todo.notes.map((note) => note.body),
  ].some((value) => normalizeSearch(value).includes(query));
}

function normalizeSearch(value: string) {
  return value.trim().toLocaleLowerCase();
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function plainTextSummary(value: string) {
  return value
    .replace(/!\[[^\]]*]\([^)]*\)/g, "图片")
    .replace(/\[([^\]]+)]\([^)]*\)/g, "$1")
    .replace(/[`*_~>#-]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 120);
}

function toTimestamp(value?: string | null) {
  if (!value) return Number.POSITIVE_INFINITY;
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? Number.POSITIVE_INFINITY : time;
}

function formatTodoDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function toDateTimeLocalValue(value?: string | Date | null) {
  if (!value) return "";
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";

  const offset = date.getTimezoneOffset();
  const localDate = new Date(date.getTime() - offset * 60_000);
  return localDate.toISOString().slice(0, 16);
}

function toIsoDateTime(value: string) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

function isOverdue(value?: string | null) {
  return Boolean(value && new Date(value).getTime() < Date.now());
}

function isDueSoon(value?: string | null) {
  if (!value) return false;
  const time = new Date(value).getTime();
  if (Number.isNaN(time)) return false;
  const now = Date.now();
  return time >= now && time - now <= 24 * 60 * 60 * 1000;
}

function dueBadgeClass(todo: TodoItem) {
  if (todo.completed) return "bg-foreground/5 text-muted-foreground";
  if (isOverdue(todo.due_at)) return "bg-rose-500/12 text-rose-600 dark:text-rose-300";
  if (isDueSoon(todo.due_at)) return "bg-amber-400/16 text-amber-700 dark:text-amber-300";
  return "bg-primary/10 text-primary";
}

async function imagesFromClipboard(event: ClipboardEvent<HTMLElement>) {
  const files = Array.from(event.clipboardData.items)
    .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file));

  if (files.length === 0) return [];

  const images: DraftImage[] = [];
  for (const file of files) {
    const image = await createDraftImageFromBlob(file);
    if (image) images.push(image);
  }

  return images;
}

async function createDraftImageFromBlob(blob: Blob) {
  if (blob.size > MAX_IMAGE_BYTES) {
    toast.error("单张图片不能超过 5MB");
    return null;
  }

  if (!["image/png", "image/jpeg", "image/webp", "image/gif"].includes(blob.type)) {
    toast.error("仅支持 PNG、JPEG、WebP 或 GIF 图片");
    return null;
  }

  return {
    local_id: createLocalId(),
    data_url: await readFileAsDataUrl(blob),
    mime_type: blob.type,
  };
}

function readFileAsDataUrl(file: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("图片读取失败"));
    reader.readAsDataURL(file);
  });
}

function toTodoImageInput(image: DraftImage): TodoImageInput {
  return {
    data_url: image.data_url,
    mime_type: image.mime_type,
  };
}

function createLocalId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败";
}
