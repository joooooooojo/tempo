import { useEffect, useMemo, useRef, useState, type ClipboardEvent, type FormEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  CheckCircle2,
  Circle,
  Download,
  ImagePlus,
  Pin,
  Search,
  Timer,
  Upload,
  X,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { MarkdownPreview } from "@/components/todos/MarkdownPreview";
import { TodoCreateDialog } from "@/components/todos/TodoCreateDialog";
import { TodoSubtaskList } from "@/components/todos/TodoSubtasks";
import { TodoTagList } from "@/components/todos/TodoTags";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { recurrenceLabel, subtaskProgress, todoReminderLabel } from "@/lib/todoMeta";
import { cn, formatDurationShort } from "@/lib/utils";
import type { TodoImage, TodoItem, TodoNote, TodoRecurrence, TodoSubtask, TodoFocusSummary } from "@/types";
import { TodoPagination } from "./TodoPagination";
import {
  HighlightText,
  ImagePreviewViewport,
  NoteComposer,
  TodoEmptyState,
  TodoExpandableSection,
  TodoFocusStats,
  TodoImages,
  TodoNotes,
  TodoRowActionMenu,
  TodoStat,
  type NoteDraft,
} from "./TodoListSections";
import {
  emptyText,
  ensureTodoDetails,
  errorMessage,
  filterCount,
  formatTodoDate,
  imagesFromClipboard,
  isDueSoon,
  matchesTodoSearch,
  MAX_IMAGES_PER_NOTE,
  normalizeSearch,
  normalizeTodo,
  replaceTodo,
  sortTodos,
  toDateTimeLocalValue,
  toIsoDateTime,
  toTodoImageInput,
  todoImageCount,
  todoSummary,
  TODO_PAGE_SIZE,
  dueBadgeClass,
  upsertTodo,
  type TodoFilter,
} from "./todoPageUtils";

interface PreviewImage {
  data_url: string;
  label: string;
}

const filters: Array<{ value: TodoFilter; label: string }> = [
  { value: "active", label: "未完成" },
  { value: "completed", label: "已完成" },
];

export function TodoPage() {
  const navigate = useNavigate();
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [recurrence, setRecurrence] = useState<TodoRecurrence>("none");
  const [remind1d, setRemind1d] = useState(false);
  const [remind1h, setRemind1h] = useState(false);
  const [remindCustomHours, setRemindCustomHours] = useState<number | null>(null);
  const [subtasks, setSubtasks] = useState<string[]>([]);
  const [tags, setTags] = useState<string[]>([]);
  const [noteDrafts, setNoteDrafts] = useState<Record<number, NoteDraft>>({});
  const [createOpen, setCreateOpen] = useState(false);
  const [detailId, setDetailId] = useState<number | null>(null);
  const [previewImage, setPreviewImage] = useState<PreviewImage | null>(null);
  const closingPreviewRef = useRef(false);

  const closePreviewImage = () => {
    closingPreviewRef.current = true;
    setPreviewImage(null);
    window.setTimeout(() => {
      closingPreviewRef.current = false;
    }, 100);
  };

  const shouldKeepUnderlyingDialog = () =>
    Boolean(previewImage) || closingPreviewRef.current;
  const [filter, setFilter] = useState<TodoFilter>("active");
  const [searchQuery, setSearchQuery] = useState("");
  const [page, setPage] = useState(1);
  const [actionMenuId, setActionMenuId] = useState<number | null>(null);
  const [expandedTodoIds, setExpandedTodoIds] = useState<Set<number>>(() => new Set());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editContent, setEditContent] = useState("");
  const [editDueAt, setEditDueAt] = useState("");
  const [editRecurrence, setEditRecurrence] = useState<TodoRecurrence>("none");
  const [editRemind1d, setEditRemind1d] = useState(false);
  const [editRemind1h, setEditRemind1h] = useState(false);
  const [editRemindCustomHours, setEditRemindCustomHours] = useState<number | null>(null);
  const [editTags, setEditTags] = useState<string[]>([]);
  const [focusSummaries, setFocusSummaries] = useState<Record<number, TodoFocusSummary>>({});
  const [detailFocusSummary, setDetailFocusSummary] = useState<TodoFocusSummary | null>(null);

  useEffect(() => {
    let cancelled = false;

    api.getTodos()
      .then((items) => {
        if (!cancelled) setTodos(sortTodos(items.map(normalizeTodo)));
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
      const created = normalizeTodo(event.payload);
      setTodos((current) => sortTodos([created, ...current.filter((todo) => todo.id !== created.id)]));
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

  const totalPages = Math.max(1, Math.ceil(visibleTodos.length / TODO_PAGE_SIZE));
  const showPagination = visibleTodos.length > TODO_PAGE_SIZE;
  const paginatedTodos = useMemo(() => {
    const start = (page - 1) * TODO_PAGE_SIZE;
    return visibleTodos.slice(start, start + TODO_PAGE_SIZE);
  }, [page, visibleTodos]);

  useEffect(() => {
    setPage(1);
  }, [filter, searchQuery]);

  useEffect(() => {
    if (page > totalPages) setPage(totalPages);
  }, [page, totalPages]);

  const tagSuggestions = useMemo(() => {
    const seen = new Set<string>();
    const next: string[] = [];
    for (const todo of todos) {
      for (const tag of todo.tags) {
        const key = tag.toLocaleLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);
        next.push(tag);
      }
    }
    return next.sort((a, b) => a.localeCompare(b, "zh-CN"));
  }, [todos]);
  const editingTodo = useMemo(
    () => (editingId === null ? null : todos.find((todo) => todo.id === editingId) ?? null),
    [editingId, todos]
  );
  const detailTodo = useMemo(
    () => (detailId === null ? null : todos.find((todo) => todo.id === detailId) ?? null),
    [detailId, todos]
  );

  const refreshFocusSummaries = async (todoIds: number[]) => {
    if (todoIds.length === 0) return;
    const summaries = await api.getTodoFocusSummaries(todoIds);
    setFocusSummaries((current) => {
      const next = { ...current };
      for (const summary of summaries) {
        next[summary.todo_id] = summary;
      }
      return next;
    });
  };

  useEffect(() => {
    if (filter !== "active") return;
    void refreshFocusSummaries(visibleTodos.map((todo) => todo.id)).catch(console.error);
  }, [filter, visibleTodos]);

  useEffect(() => {
    if (!detailTodo) {
      setDetailFocusSummary(null);
      return;
    }
    void api.getTodoFocusSummary(detailTodo.id)
      .then(setDetailFocusSummary)
      .catch(console.error);
  }, [detailTodo?.id]);

  useEffect(() => {
    const unlisten = listen<{ type: string; phase?: string; skipped?: boolean }>("reminder", (event) => {
      if (
        event.payload.type !== "pomodoro_phase_end" ||
        event.payload.phase !== "work" ||
        event.payload.skipped
      ) {
        return;
      }

      const activeIds = todos.filter((todo) => !todo.completed).map((todo) => todo.id);
      void refreshFocusSummaries(activeIds).catch(console.error);
      if (detailId !== null) {
        void api.getTodoFocusSummary(detailId).then(setDetailFocusSummary).catch(console.error);
      }
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [detailId, todos]);

  const openDetail = (todoId: number) => {
    setDetailId(todoId);
    void ensureTodoDetails(todoId, setTodos);
  };

  const startFocusForTodo = async (todo: TodoItem) => {
    try {
      const current = await api.getPomodoroState();
      if (current.status !== "idle") {
        toast.error("已有进行中的番茄钟");
        navigate("/pomodoro");
        return;
      }
      await api.startPomodoro(todo.id);
      navigate("/pomodoro");
    } catch (error) {
      console.error(error);
      toast.error(error instanceof Error ? error.message : "无法开始专注");
    }
  };

  const resetDraft = () => {
    setTitle("");
    setContent("");
    setDueAt("");
    setRecurrence("none");
    setRemind1d(false);
    setRemind1h(false);
    setRemindCustomHours(null);
    setSubtasks([]);
    setTags([]);
  };

  const handleRecurrenceChange = (value: TodoRecurrence) => {
    setRecurrence(value);
    if (value !== "none") {
      setDueAt("");
      setRemind1d(false);
      setRemind1h(false);
      setRemindCustomHours(null);
    }
  };

  const handleEditRecurrenceChange = (value: TodoRecurrence) => {
    setEditRecurrence(value);
    if (value !== "none") {
      setEditDueAt("");
      setEditRemind1d(false);
      setEditRemind1h(false);
      setEditRemindCustomHours(null);
    }
  };

  const handleDueAtChange = (value: string) => {
    setDueAt(value);
    if (value) {
      setRecurrence("none");
    }
    if (!value) {
      setRemind1d(false);
      setRemind1h(false);
      setRemindCustomHours(null);
    }
  };

  const handleEditDueAtChange = (value: string) => {
    setEditDueAt(value);
    if (value) {
      setEditRecurrence("none");
    }
    if (!value) {
      setEditRemind1d(false);
      setEditRemind1h(false);
      setEditRemindCustomHours(null);
    }
  };

  const applyTodoUpdate = (todo: TodoItem) => {
    setTodos((current) => replaceTodo(current, normalizeTodo(todo)));
  };

  const toggleTodoExpanded = (todo: TodoItem, hasExpandableContent: boolean) => {
    const willExpand = !expandedTodoIds.has(todo.id);
    setExpandedTodoIds((current) => {
      const next = new Set(current);
      if (next.has(todo.id)) next.delete(todo.id);
      else next.add(todo.id);
      return next;
    });
    if (willExpand && hasExpandableContent) {
      void ensureTodoDetails(todo.id, setTodos);
    }
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
      setTodos(sortTodos(items.map(normalizeTodo)));
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
      setExpandedTodoIds((current) => new Set(current).add(todo.id));
      updateNoteDraft(todo.id, () => ({ body: "", images: [], open: true }));
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
        toIsoDateTime(dueAt),
        [],
        recurrence,
        remind1d,
        remind1h,
        remindCustomHours,
        subtasks,
        tags
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
      applyTodoUpdate(updated);
      toast.success(
        updated.completed
          ? todo.recurrence !== "none"
            ? "已完成，下一周期将在开始时自动创建"
            : "已完成"
          : "已恢复", {
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
    setEditRecurrence(todo.recurrence);
    setEditRemind1d(todo.remind_1d);
    setEditRemind1h(todo.remind_1h);
    setEditRemindCustomHours(todo.remind_custom_hours ?? null);
    setEditTags(todo.tags);
    void ensureTodoDetails(todo.id, setTodos);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditTitle("");
    setEditContent("");
    setEditDueAt("");
    setEditRecurrence("none");
    setEditRemind1d(false);
    setEditRemind1h(false);
    setEditRemindCustomHours(null);
    setEditTags([]);
  };

  const handleEditOpenChange = (nextOpen: boolean) => {
    if (saving) return;
    if (!nextOpen) {
      if (shouldKeepUnderlyingDialog()) return;
      cancelEdit();
    }
  };

  const commitEdit = async (todo: TodoItem) => {
    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      toast.error("请输入标题");
      return;
    }

    setSaving(true);
    try {
      const updated = await api.updateTodoDetails(
        todo.id,
        nextTitle,
        editContent,
        toIsoDateTime(editDueAt),
        editRecurrence,
        editRemind1d,
        editRemind1h,
        editRemindCustomHours,
        editTags
      );
      setTodos((current) => replaceTodo(current, updated));
      cancelEdit();
      toast.success("已更新");
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const toggleSubtask = async (subtask: TodoSubtask, completed: boolean) => {
    try {
      const updated = await api.setTodoSubtaskCompleted(subtask.id, completed);
      applyTodoUpdate(updated);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const addSubtask = async (todoId: number, title: string) => {
    try {
      const updated = await api.addTodoSubtask(todoId, title);
      applyTodoUpdate(updated);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const deleteSubtask = async (subtask: TodoSubtask) => {
    try {
      const updated = await api.deleteTodoSubtask(subtask.id);
      applyTodoUpdate(updated);
    } catch (error) {
      toast.error(errorMessage(error));
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
    <div className="mx-auto flex h-full min-h-0 w-full max-w-3xl flex-col gap-5">
      <div className="todo-stats-row grid shrink-0 grid-cols-4 gap-3">
        <TodoStat label="未完成" value={activeCount} />
        <TodoStat label="即将截止" value={dueSoonCount} tone={dueSoonCount > 0 ? "warning" : "default"} />
        <TodoStat label="已完成" value={completedCount} />
        <Card>
          <CardContent className="p-3.5">
            <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              完成率
            </p>
            <div className="mt-2 flex items-center gap-3">
              <p className="stat-value w-12 text-xl font-bold text-primary">{completionRate}%</p>
              <div className="progress-track h-1.5 min-w-0 flex-1 overflow-hidden rounded-sm bg-foreground/8">
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
        recurrence={recurrence}
        remind1d={remind1d}
        remind1h={remind1h}
        remindCustomHours={remindCustomHours}
        subtasks={subtasks}
        tags={tags}
        tagSuggestions={tagSuggestions}
        saving={saving}
        onOpenChange={handleCreateOpenChange}
        onTitleChange={setTitle}
        onContentChange={setContent}
        onDueAtChange={handleDueAtChange}
        onRecurrenceChange={handleRecurrenceChange}
        onRemind1dChange={setRemind1d}
        onRemind1hChange={setRemind1h}
        onRemindCustomHoursChange={setRemindCustomHours}
        onSubtasksChange={setSubtasks}
        onTagsChange={setTags}
        onSubmit={handleAdd}
      />

      <TodoCreateDialog
        open={Boolean(editingTodo)}
        heading="编辑待办事项"
        todoTitle={editTitle}
        todoContent={editContent}
        dueAt={editDueAt}
        recurrence={editRecurrence}
        remind1d={editRemind1d}
        remind1h={editRemind1h}
        remindCustomHours={editRemindCustomHours}
        tags={editTags}
        tagSuggestions={tagSuggestions}
        saving={saving}
        submitLabel="保存"
        bodyExtra={
          <>
            {editingTodo && todoImageCount(editingTodo) > 0 ? (
              <TodoImages
                images={editingTodo.images}
                onDelete={deleteImage}
                onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: editingTodo.title })}
              />
            ) : null}
            {editingTodo && (
              <TodoSubtaskList
                subtasks={editingTodo.subtasks}
                editable
                onToggle={toggleSubtask}
                onDelete={deleteSubtask}
                onAdd={(value) => void addSubtask(editingTodo.id, value)}
              />
            )}
          </>
        }
        onOpenChange={handleEditOpenChange}
        onTitleChange={setEditTitle}
        onContentChange={setEditContent}
        onDueAtChange={handleEditDueAtChange}
        onRecurrenceChange={handleEditRecurrenceChange}
        onRemind1dChange={setEditRemind1d}
        onRemind1hChange={setEditRemind1h}
        onRemindCustomHoursChange={setEditRemindCustomHours}
        onTagsChange={setEditTags}
        onSubmit={(event) => {
          event.preventDefault();
          if (editingTodo) void commitEdit(editingTodo);
        }}
      />

      <Dialog
        open={Boolean(detailTodo)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) {
            if (shouldKeepUnderlyingDialog()) return;
            setDetailId(null);
          }
        }}
      >
        <DialogContent
          showCloseButton={false}
          className="todo-create-dialog max-w-[680px] gap-0 overflow-hidden rounded-xl border-border/80 p-0"
        >
          {detailTodo && (
            <>
              <div className="flex items-center gap-2 border-b border-border/60 px-5 py-3">
                <DialogTitle
                  className="m-0 flex h-8 min-w-0 flex-1 items-center overflow-hidden p-0 text-left text-[18px] font-bold leading-none"
                  title={detailTodo.title}
                >
                  <span className="block w-full truncate leading-none">
                    <HighlightText value={detailTodo.title} query={searchQuery} />
                  </span>
                </DialogTitle>
                {!detailTodo.completed && (
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-8 shrink-0 gap-1 border-emerald-500/30 bg-emerald-500/8 px-2.5 text-[12px] text-emerald-700 shadow-none transition-[background,border-color,box-shadow,color] hover:border-emerald-500/60 hover:bg-emerald-500/28 hover:text-emerald-900 hover:shadow-[0_0_0_3px_rgba(16,185,129,0.18)] dark:border-emerald-400/30 dark:bg-emerald-500/12 dark:text-emerald-300 dark:hover:border-emerald-300/55 dark:hover:bg-emerald-500/35 dark:hover:text-emerald-100 dark:hover:shadow-[0_0_0_3px_rgba(52,211,153,0.2)]"
                    onClick={() => {
                      setDetailId(null);
                      void startFocusForTodo(detailTodo);
                    }}
                  >
                    <Timer className="h-3.5 w-3.5" />
                    专注
                  </Button>
                )}
                <DialogClose asChild>
                  <button
                    type="button"
                    className="group relative inline-flex h-8 w-8 shrink-0 items-center justify-center text-muted-foreground opacity-55 transition-opacity hover:opacity-100 focus:outline-none"
                    aria-label="关闭"
                  >
                    <span
                      aria-hidden="true"
                      className="absolute left-1/2 top-1/2 h-[26px] w-[26px] -translate-x-1/2 -translate-y-1/2 rounded-md bg-transparent transition-[background,transform] duration-150 group-hover:scale-90 group-hover:bg-foreground/10 dark:group-hover:bg-white/12"
                    />
                    <X className="relative h-3.5 w-3.5" />
                  </button>
                </DialogClose>
              </div>
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
                  {detailTodo.recurrence !== "none" && (
                    <span className="rounded-md bg-primary/10 px-1.5 py-0.5 font-medium text-primary">
                      {recurrenceLabel(detailTodo.recurrence)}
                    </span>
                  )}
                  {todoReminderLabel(detailTodo) && (
                    <span className="rounded-md bg-foreground/5 px-1.5 py-0.5 font-medium">
                      {todoReminderLabel(detailTodo)}
                    </span>
                  )}
                </div>

                {detailTodo.tags.length > 0 && (
                  <div className="mb-4">
                    <TodoTagList tags={detailTodo.tags} />
                  </div>
                )}

                {detailFocusSummary && detailFocusSummary.sessions_all > 0 && (
                  <TodoFocusStats summary={detailFocusSummary} />
                )}

                <MarkdownPreview
                  value={detailTodo.content}
                  onImagePreview={(src, alt) => setPreviewImage({ data_url: src, label: alt })}
                />

                <TodoSubtaskList subtasks={detailTodo.subtasks} readOnly />

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
          if (!nextOpen) closePreviewImage();
        }}
      >
        <DialogContent className="flex h-[90vh] w-[90vw] max-w-[90vw] flex-col gap-3 overflow-hidden p-3">
          <DialogHeader className="shrink-0 px-1 pr-8">
            <DialogTitle className="truncate text-[15px]">图片预览</DialogTitle>
          </DialogHeader>
          {previewImage && (
            <ImagePreviewViewport src={previewImage.data_url} alt={previewImage.label} />
          )}
        </DialogContent>
      </Dialog>

      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
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
        </div>

        <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
          <label className="relative min-w-[180px] max-w-[280px] flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <input
              value={searchQuery}
              placeholder="搜索待办、标签、正文、备注"
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

      <div className="min-h-0 flex-1">
        <Card className="flex h-fit max-h-full w-full flex-col overflow-hidden">
          <CardContent className="no-scrollbar min-h-0 flex-1 overflow-y-auto p-0">
          {loading ? (
            <TodoEmptyState text="加载中..." />
          ) : visibleTodos.length === 0 ? (
            <TodoEmptyState text={emptyText(filter, Boolean(searchQuery.trim()))} />
          ) : (
            <div className="divide-y divide-border/45">
              {paginatedTodos.map((todo) => {
                const noteDraft = noteDrafts[todo.id] ?? { body: "", images: [] };
                const summary = todoSummary(todo);
                const checklist = subtaskProgress(todo.subtasks);
                const imageCount = todoImageCount(todo);
                const hasExpandableContent = Boolean(
                  (todo.subtasks.length > 0 && !todo.completed) ||
                    summary ||
                    imageCount > 0 ||
                    todo.notes.length > 0
                );
                const isExpanded = expandedTodoIds.has(todo.id) || Boolean(noteDraft.open);

                return (
                  <div
                    key={todo.id}
                    className={cn(
                      "group/todo relative px-4 py-3 transition-colors hover:bg-foreground/[0.03] focus-within:bg-foreground/[0.03]",
                      todo.completed && "bg-foreground/[0.018]"
                    )}
                  >
                    <div
                      className={cn(
                        "grid grid-cols-[36px_minmax(0,1fr)_auto] items-center gap-x-3",
                        (hasExpandableContent || noteDraft.open) && "cursor-pointer"
                      )}
                      onClick={() => {
                        if (!hasExpandableContent && !noteDraft.open) return;
                        toggleTodoExpanded(todo, hasExpandableContent);
                      }}
                    >
                      <button
                        type="button"
                        className="flex h-9 w-9 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-primary"
                        aria-label={todo.completed ? "恢复待办" : "完成待办"}
                        onClick={(event) => {
                          event.stopPropagation();
                          void toggleTodo(todo);
                        }}
                      >
                        {todo.completed ? (
                          <CheckCircle2 className="h-5 w-5 text-primary" />
                        ) : (
                          <Circle className="h-5 w-5" />
                        )}
                      </button>

                      <div className="min-w-0 text-left">
                          <div className="flex min-w-0 items-center gap-2">
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
                              onClick={(event) => {
                                event.stopPropagation();
                                openDetail(todo.id);
                              }}
                            >
                              <HighlightText value={todo.title} query={searchQuery} />
                            </button>
                            {imageCount > 0 && (
                              <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-foreground/5 px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground">
                                <ImagePlus className="h-3 w-3" />
                                {imageCount}
                              </span>
                            )}
                          </div>
                          <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
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
                            {todo.recurrence !== "none" && (
                              <span className="rounded-md bg-primary/10 px-1.5 py-0.5 font-medium text-primary">
                                {recurrenceLabel(todo.recurrence)}
                              </span>
                            )}
                            {checklist && (
                              <span className="rounded-md bg-foreground/5 px-1.5 py-0.5 font-medium">
                                子任务 {checklist}
                              </span>
                            )}
                            {todo.tags.length > 0 && (
                              <TodoTagList tags={todo.tags} compact />
                            )}
                            {todo.notes.length > 0 && (
                              <span className="rounded-md bg-foreground/5 px-1.5 py-0.5 font-medium">
                                备注 {todo.notes.length}
                              </span>
                            )}
                            {focusSummaries[todo.id]?.sessions_today ? (
                              <span className="rounded-md bg-emerald-500/10 px-1.5 py-0.5 font-medium text-emerald-600 dark:text-emerald-300">
                                今日专注 {focusSummaries[todo.id].sessions_today} 轮 · {formatDurationShort(focusSummaries[todo.id].total_seconds_today)}
                              </span>
                            ) : null}
                          </div>
                      </div>

                      <div
                        className="flex items-center self-center gap-1"
                        onClick={(event) => event.stopPropagation()}
                      >
                        {!todo.completed && (
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon"
                            className="h-8 w-8 shrink-0 p-0 text-muted-foreground opacity-100 hover:bg-emerald-500/10 hover:text-emerald-600 sm:opacity-0 sm:group-hover/todo:opacity-100 sm:group-focus-within/todo:opacity-100 dark:hover:text-emerald-300"
                            aria-label="开始专注"
                            title="开始专注"
                            onClick={() => void startFocusForTodo(todo)}
                          >
                            <Timer className="h-4 w-4 shrink-0" />
                          </Button>
                        )}
                        <TodoRowActionMenu
                            open={actionMenuId === todo.id}
                            onOpenChange={(open) => setActionMenuId(open ? todo.id : null)}
                            showStartFocus={!todo.completed}
                            onStartFocus={() => {
                              setActionMenuId(null);
                              void startFocusForTodo(todo);
                            }}
                            onAddNote={() => {
                              setActionMenuId(null);
                              setExpandedTodoIds((current) => new Set(current).add(todo.id));
                              updateNoteDraft(todo.id, (current) => ({ ...current, open: true }));
                              void ensureTodoDetails(todo.id, setTodos);
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
                    </div>
                    {(hasExpandableContent || noteDraft.open) && (
                      <TodoExpandableSection open={isExpanded}>
                        {todo.subtasks.length > 0 && !todo.completed && (
                          <TodoSubtaskList
                            subtasks={todo.subtasks}
                            compact
                            onToggle={toggleSubtask}
                          />
                        )}
                        {summary && (
                          <button
                            type="button"
                            className="mt-1.5 block max-w-full truncate text-left text-[12px] leading-5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                            onClick={(event) => {
                              event.stopPropagation();
                              openDetail(todo.id);
                            }}
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
                      </TodoExpandableSection>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
        {showPagination && (
          <TodoPagination
            page={page}
            totalPages={totalPages}
            totalItems={visibleTodos.length}
            pageSize={TODO_PAGE_SIZE}
            onPageChange={setPage}
          />
        )}
        </Card>
      </div>
    </div>
  );
}
