import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEventHandler,
  type ClipboardEvent,
  type FormEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  CalendarClock,
  CheckCircle2,
  Circle,
  ClipboardList,
  ImagePlus,
  MessageSquarePlus,
  Pencil,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Card, CardContent } from "@/components/ui/card";
import { TodoCreateDialog, type DraftTodoImage } from "@/components/todos/TodoCreateDialog";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
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

const MAX_IMAGES_PER_TODO = 4;
const MAX_IMAGES_PER_NOTE = 4;
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const DEFAULT_DUE_HOUR = "18";
const DEFAULT_DUE_MINUTE = "00";
const hourOptions = ["08", "09", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "22", "23"];
const minuteOptions = ["00", "15", "30", "45"];

export function TodoPage() {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [title, setTitle] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [draftImages, setDraftImages] = useState<DraftImage[]>([]);
  const [noteDrafts, setNoteDrafts] = useState<Record<number, NoteDraft>>({});
  const [createOpen, setCreateOpen] = useState(false);
  const [previewImage, setPreviewImage] = useState<PreviewImage | null>(null);
  const [filter, setFilter] = useState<TodoFilter>("active");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");
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
    if (filter === "active") return todos.filter((todo) => !todo.completed);
    return todos.filter((todo) => todo.completed);
  }, [filter, todos]);
  const editingTodo = useMemo(
    () => (editingId === null ? null : todos.find((todo) => todo.id === editingId) ?? null),
    [editingId, todos]
  );

  const resetDraft = () => {
    setTitle("");
    setDueAt("");
    setDraftImages([]);
  };

  const handleCreateOpenChange = (nextOpen: boolean) => {
    if (saving) return;

    setCreateOpen(nextOpen);
    if (!nextOpen) resetDraft();
  };

  const handlePagePaste = async (event: ClipboardEvent<HTMLDivElement>) => {
    const hasPastedImage = Array.from(event.clipboardData.items).some(
      (item) => item.kind === "file" && item.type.startsWith("image/")
    );
    if (!hasPastedImage) return;

    event.preventDefault();
    event.stopPropagation();

    const pastedImages = await imagesFromClipboard(event);
    if (pastedImages.length === 0) return;

    if (editingId !== null) {
      const target = todos.find((todo) => todo.id === editingId);
      if (!target) return;

      const available = MAX_IMAGES_PER_TODO - target.images.length;
      if (available <= 0) {
        toast.error(`每个待办最多添加 ${MAX_IMAGES_PER_TODO} 张图片`);
        return;
      }

      await addImagesToExistingTodo(target, pastedImages.slice(0, available));
      if (pastedImages.length > available) toast.info("已达到图片数量上限");
      return;
    }

    const available = MAX_IMAGES_PER_TODO - draftImages.length;
    if (available <= 0) {
      toast.error(`每个待办最多添加 ${MAX_IMAGES_PER_TODO} 张图片`);
      return;
    }

    setCreateOpen(true);
    setDraftImages((current) => [...current, ...pastedImages.slice(0, available)]);
    toast.success(pastedImages.length > available ? "已添加部分图片" : "已粘贴图片");
  };

  const addImagesToExistingTodo = async (todo: TodoItem, images: DraftImage[]) => {
    try {
      let updated = todo;
      for (const image of images) {
        updated = await api.addTodoImage(todo.id, toTodoImageInput(image));
      }
      setTodos((current) => replaceTodo(current, updated));
      toast.success(images.length > 1 ? `已添加 ${images.length} 张图片` : "已粘贴图片");
    } catch (error) {
      toast.error(errorMessage(error));
    }
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
      toast.success("备注已删除");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const handleAdd = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!title.trim() && draftImages.length === 0) {
      toast.error("请输入待办内容");
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(
        title,
        toIsoDateTime(dueAt),
        draftImages.map(toTodoImageInput)
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
      toast.success(updated.completed ? "已完成" : "已恢复");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const startEdit = (todo: TodoItem) => {
    setEditingId(todo.id);
    setEditTitle(todo.title);
    setEditDueAt(toDateTimeLocalValue(todo.due_at));
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditTitle("");
    setEditDueAt("");
  };

  const handleEditOpenChange = (nextOpen: boolean) => {
    if (saving) return;
    if (!nextOpen) cancelEdit();
  };

  const commitEdit = async (todo: TodoItem) => {
    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      toast.error("请输入待办内容");
      return;
    }

    try {
      const updated = await api.updateTodoDetails(todo.id, nextTitle, toIsoDateTime(editDueAt));
      setTodos((current) => replaceTodo(current, updated));
      cancelEdit();
      toast.success("已更新");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const deleteTodo = async (todo: TodoItem) => {
    try {
      await api.deleteTodo(todo.id);
      setTodos((current) => current.filter((item) => item.id !== todo.id));
      toast.success("已删除");
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
    <div className="mx-auto max-w-3xl space-y-5" onPaste={handlePagePaste}>
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
        dueAt={dueAt}
        images={draftImages}
        saving={saving}
        onOpenChange={handleCreateOpenChange}
        onTitleChange={setTitle}
        onDueAtChange={setDueAt}
        onDeleteImage={(image) =>
          setDraftImages((current) => current.filter((item) => item.local_id !== image.local_id))
        }
        onSubmit={handleAdd}
      />

      <Dialog open={Boolean(editingTodo)} onOpenChange={handleEditOpenChange}>
        <DialogContent className="todo-create-dialog max-w-[520px] gap-0 overflow-visible rounded-xl border-border/80 p-0">
          <DialogHeader className="border-b border-border/60 px-5 py-4 pr-12">
            <DialogTitle className="text-[18px] font-bold">编辑待办</DialogTitle>
          </DialogHeader>
          {editingTodo && (
            <form
              className="contents"
              onSubmit={(event) => {
                event.preventDefault();
                void commitEdit(editingTodo);
              }}
            >
              <div className="space-y-4 px-5 pb-4 pt-5">
                <div>
                  <DueDateField
                    value={editDueAt}
                    className="h-10 w-full"
                    floatingLabel
                    popoverPortalled={false}
                    onChange={setEditDueAt}
                  />
                </div>

                <div>
                  <FloatingTextarea
                    id="edit-todo-title"
                    autoFocus
                    value={editTitle}
                    maxLength={120}
                    placeholder="待办内容"
                    onChange={(event) => setEditTitle(event.target.value)}
                  />
                </div>

                <div>
                  {editingTodo.images.length > 0 ? (
                    <TodoImages
                      images={editingTodo.images}
                      onDelete={deleteImage}
                      onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: editingTodo.title })}
                    />
                  ) : (
                    <div className="flex h-14 items-center justify-center gap-2 rounded-lg border border-dashed border-border/70 bg-background/46 text-muted-foreground">
                      <ImagePlus className="h-4 w-4" />
                      <span className="text-[13px]">暂无图片</span>
                    </div>
                  )}
                </div>
              </div>

              <DialogFooter className="gap-2 border-t border-border/60 bg-foreground/[0.018] px-5 py-4 sm:space-x-0">
                <DialogClose asChild>
                  <Button type="button" variant="outline" className="h-9 min-w-20">
                    取消
                  </Button>
                </DialogClose>
                <Button type="submit" className="h-9 min-w-24" disabled={saving}>
                  保存
                </Button>
              </DialogFooter>
            </form>
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

      <div className="flex items-center justify-between gap-3">
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

        <Button
          size="sm"
          className="h-9 px-4"
          onClick={() => setCreateOpen(true)}
        >
          新建
        </Button>
      </div>

      <Card className="overflow-hidden">
        <CardContent className="p-0">
          {loading ? (
            <TodoEmptyState text="加载中..." />
          ) : visibleTodos.length === 0 ? (
            <TodoEmptyState text={emptyText(filter)} />
          ) : (
            <div className="divide-y divide-border/45">
              {visibleTodos.map((todo) => {
                const noteDraft = noteDrafts[todo.id] ?? { body: "", images: [] };

                return (
                  <div
                    key={todo.id}
                    className={cn(
                      "grid grid-cols-[36px_minmax(0,1fr)_auto] items-start gap-3 px-4 py-3.5 transition-colors hover:bg-foreground/[0.03]",
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
                      <div className="flex min-w-0 items-center gap-2">
                        <p
                          className={cn(
                            "truncate text-[14px] font-semibold transition-colors",
                            todo.completed && "text-muted-foreground line-through"
                          )}
                        >
                          {todo.title}
                        </p>
                        {todo.images.length > 0 && (
                          <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-foreground/5 px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground">
                            <ImagePlus className="h-3 w-3" />
                            {todo.images.length}
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
                      </div>
                      <TodoImages
                        images={todo.images}
                        onPreview={(image) => setPreviewImage({ data_url: image.data_url, label: todo.title })}
                      />
                      <TodoNotes
                        notes={todo.notes}
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

                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-muted-foreground"
                        aria-label="追加备注"
                        onClick={() => updateNoteDraft(todo.id, (current) => ({ ...current, open: true }))}
                      >
                        <MessageSquarePlus className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-muted-foreground"
                        aria-label="编辑"
                        onClick={() => startEdit(todo)}
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-muted-foreground hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-300"
                        aria-label="删除"
                        onClick={() => void deleteTodo(todo)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
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

function FloatingTextarea({
  id,
  value,
  placeholder,
  maxLength,
  autoFocus,
  className,
  onChange,
  onPaste,
}: {
  id: string;
  value: string;
  placeholder: string;
  maxLength?: number;
  autoFocus?: boolean;
  className?: string;
  onChange: ChangeEventHandler<HTMLTextAreaElement>;
  onPaste?: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
}) {
  const [focused, setFocused] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const floated = focused || value.length > 0;

  useEffect(() => {
    if (!autoFocus) return;

    const frame = requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;

      const end = textarea.value.length;
      textarea.focus();
      textarea.setSelectionRange(end, end);
    });

    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        id={id}
        autoFocus={autoFocus}
        value={value}
        maxLength={maxLength}
        placeholder={floated ? "" : placeholder}
        className={cn(
          "block min-h-20 w-full resize-none rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 py-3 text-[14px] leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50",
          floated && "border-primary/45",
          className
        )}
        onChange={onChange}
        onPaste={onPaste}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      <span
        className={cn(
          "pointer-events-none absolute left-3 top-px z-10 origin-left -translate-y-1/2 rounded-sm bg-[var(--todo-field-bg)] px-1 text-[11px] font-medium leading-none transition-all duration-150",
          floated
            ? cn("scale-100 opacity-100", focused ? "text-primary" : "text-muted-foreground")
            : "scale-95 opacity-0"
        )}
      >
        {placeholder}
      </span>
    </div>
  );
}

function DueDateField({
  value,
  className,
  floatingLabel = false,
  popoverPortalled = true,
  onChange,
}: {
  value: string;
  className?: string;
  floatingLabel?: boolean;
  popoverPortalled?: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const selectedDate = parseDateTimeLocalValue(value);
  const [visibleMonth, setVisibleMonth] = useState(() => startOfMonth(selectedDate ?? new Date()));
  const fallbackDate = getDefaultDueDate(new Date());
  const hour = selectedDate ? String(selectedDate.getHours()).padStart(2, "0") : String(fallbackDate.getHours()).padStart(2, "0");
  const minute = selectedDate
    ? String(Math.min(45, Math.floor(selectedDate.getMinutes() / 15) * 15)).padStart(2, "0")
    : String(fallbackDate.getMinutes()).padStart(2, "0");

  useEffect(() => {
    if (open) setVisibleMonth(startOfMonth(selectedDate ?? new Date()));
  }, [open, value]);

  const commit = (date: Date, nextHour = hour, nextMinute = minute) => {
    const next = resolveDueDate(date, nextHour, nextMinute);
    if (!next) return;
    onChange(toDateTimeLocalValue(next));
  };

  const baseDate = selectedDate ?? fallbackDate;
  const commitAndClose = (date: Date, nextHour = hour, nextMinute = minute) => {
    const next = resolveDueDate(date, nextHour, nextMinute);
    if (!next) return;
    onChange(toDateTimeLocalValue(next));
    setOpen(false);
  };
  const placeholder = "截止时间";
  const floated = floatingLabel && (open || Boolean(value));
  const isBaseDateDisabled = isDueDateDisabled(baseDate);
  const todayDisabled = isDueDateDisabled(new Date());

  return (
    <div className={cn("relative h-10 shrink-0", className)}>
      {floatingLabel && (
        <span
          className={cn(
            "pointer-events-none absolute left-3 top-px z-10 origin-left -translate-y-1/2 rounded-sm bg-[var(--todo-field-bg)] px-1 text-[11px] font-medium leading-none transition-all duration-150",
            floated
              ? cn("scale-100 opacity-100", open ? "text-primary" : "text-muted-foreground")
              : "scale-95 opacity-0"
          )}
        >
          {placeholder}
        </span>
      )}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "flex h-full w-full min-w-0 items-center gap-2 rounded-lg px-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
              floatingLabel
                ? cn(
                    "border border-border/70 bg-[var(--todo-field-bg)] shadow-sm shadow-emerald-950/[0.03] hover:brightness-[1.02]",
                    floated && "border-primary/45"
                  )
                : "glass-subtle hover:bg-white/45 dark:hover:bg-white/8",
              value ? "pr-9" : "pr-3"
            )}
            aria-label={placeholder}
          >
            <CalendarClock className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className={cn("min-w-0 truncate text-[13px] font-medium", value ? "text-foreground" : "text-muted-foreground")}>
              {value ? formatDueFieldValue(value) : floated ? "" : placeholder}
            </span>
          </button>
        </PopoverTrigger>

        {value && (
          <button
            type="button"
            className="absolute right-1.5 top-1/2 z-20 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-foreground"
            aria-label="清除截止时间"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onChange("");
              setOpen(false);
            }}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}

        <PopoverContent
          side="bottom"
          align="start"
          collisionPadding={12}
          portalled={popoverPortalled}
          className="w-[492px] overflow-hidden p-0"
        >
          <div className="grid grid-cols-[minmax(0,1fr)_132px] gap-3 p-3">
            <Calendar
              className="p-0"
              month={visibleMonth}
              selected={selectedDate}
              isDateDisabled={isDueDateDisabled}
              onMonthChange={setVisibleMonth}
              onSelect={(date) => commit(date, DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE)}
            />
            <div className="rounded-lg border border-border/60 bg-foreground/[0.025] p-2.5">
              <div>
                <p className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                  小时
                </p>
                <div className="grid grid-cols-4 gap-1">
                  {hourOptions.map((option) => {
                    const disabled = isBaseDateDisabled || isDueHourDisabled(baseDate, option);
                    return (
                      <button
                        key={option}
                        type="button"
                        disabled={disabled}
                        className={cn(
                          "h-7 rounded-md text-[12px] font-semibold transition-colors",
                          option === hour
                            ? "bg-primary text-primary-foreground shadow-sm shadow-primary/20"
                            : "bg-foreground/5 text-muted-foreground hover:bg-foreground/8 hover:text-foreground",
                          disabled && "cursor-default bg-foreground/5 text-muted-foreground/35 shadow-none hover:bg-foreground/5 hover:text-muted-foreground/35"
                        )}
                        onClick={() => commit(baseDate, option, firstSelectableMinute(baseDate, option, minute) ?? minute)}
                      >
                        {option}
                      </button>
                    );
                  })}
                </div>
              </div>
              <div className="mt-3">
                <p className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                  分钟
                </p>
                <div className="grid grid-cols-2 gap-1">
                  {minuteOptions.map((option) => {
                    const disabled = isBaseDateDisabled || isDueTimeDisabled(baseDate, hour, option);
                    return (
                      <button
                        key={option}
                        type="button"
                        disabled={disabled}
                        className={cn(
                          "h-7 rounded-md text-[12px] font-semibold transition-colors",
                          option === minute
                            ? "bg-primary text-primary-foreground shadow-sm shadow-primary/20"
                            : "bg-foreground/5 text-muted-foreground hover:bg-foreground/8 hover:text-foreground",
                          disabled && "cursor-default bg-foreground/5 text-muted-foreground/35 shadow-none hover:bg-foreground/5 hover:text-muted-foreground/35"
                        )}
                        onClick={() => commitAndClose(baseDate, hour, option)}
                      >
                        {option}
                      </button>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>
          <div className="border-t border-border/60 p-3">
            <div className="flex items-center justify-between gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-8 text-muted-foreground"
                onClick={() => {
                  onChange("");
                  setOpen(false);
                }}
              >
                清除
              </Button>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-8"
                  disabled={todayDisabled}
                  onClick={() => commit(new Date(), DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE)}
                >
                  今天
                </Button>
                <Button
                  type="button"
                  size="sm"
                  className="h-8"
                  onClick={() => {
                    if (!selectedDate || isDueTimeDisabled(selectedDate, hour, minute)) {
                      commit(
                        fallbackDate,
                        String(fallbackDate.getHours()).padStart(2, "0"),
                        String(fallbackDate.getMinutes()).padStart(2, "0")
                      );
                    }
                    setOpen(false);
                  }}
                >
                  完成
                </Button>
              </div>
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
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
  onDelete,
  onPreview,
}: {
  notes: TodoNote[];
  onDelete: (note: TodoNote) => void;
  onPreview: (image: TodoNoteImage) => void;
}) {
  if (notes.length === 0) return null;

  return (
    <div className="mt-3 space-y-2">
      {notes.map((note) => (
        <div
          key={note.id}
          className="rounded-lg border border-border/55 bg-foreground/[0.025] px-3 py-2.5"
        >
          <div className="mb-1.5 flex items-center justify-between gap-3">
            <span className="text-[11px] text-muted-foreground">
              备注 {formatTodoDate(note.created_at)}
            </span>
            <button
              type="button"
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-300"
              aria-label="删除备注"
              onClick={() => onDelete(note)}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
          {note.body && (
            <p className="whitespace-pre-wrap break-words text-[13px] leading-5 text-foreground/88">
              {note.body}
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

function emptyText(filter: TodoFilter) {
  if (filter === "completed") return "暂无已完成事项";
  return "暂无未完成事项";
}

function replaceTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos(items.map((item) => (item.id === todo.id ? todo : item)));
}

function sortTodos(items: TodoItem[]) {
  return [...items].sort((a, b) => {
    if (a.completed !== b.completed) return Number(a.completed) - Number(b.completed);

    if (!a.completed) {
      const aDue = toTimestamp(a.due_at);
      const bDue = toTimestamp(b.due_at);
      if (aDue !== bDue) return aDue - bDue;
    }

    const aTime = new Date(a.completed_at ?? a.created_at).getTime();
    const bTime = new Date(b.completed_at ?? b.created_at).getTime();
    return bTime - aTime || b.id - a.id;
  });
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

function formatDueFieldValue(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "截止时间";

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function parseDateTimeLocalValue(value?: string | null) {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function isSameLocalDay(a: Date, b: Date) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function dateWithTime(date: Date, hour: string, minute: string) {
  const next = new Date(date);
  next.setHours(Number(hour), Number(minute), 0, 0);
  return next;
}

function isDueDateDisabled(date: Date) {
  const now = new Date();
  const day = startOfDay(date);
  const today = startOfDay(now);

  if (day.getTime() < today.getTime()) return true;
  if (day.getTime() > today.getTime()) return false;

  return !minuteOptions.some((minute) =>
    hourOptions.some((hour) => dateWithTime(date, hour, minute).getTime() >= now.getTime())
  );
}

function isDueTimeDisabled(date: Date, hour: string, minute: string) {
  const now = new Date();
  if (!isSameLocalDay(date, now)) return isDueDateDisabled(date);
  return dateWithTime(date, hour, minute).getTime() < now.getTime();
}

function isDueHourDisabled(date: Date, hour: string) {
  return minuteOptions.every((minute) => isDueTimeDisabled(date, hour, minute));
}

function firstSelectableMinute(date: Date, hour: string, preferredMinute: string) {
  if (!isDueTimeDisabled(date, hour, preferredMinute)) return preferredMinute;
  return minuteOptions.find((minute) => !isDueTimeDisabled(date, hour, minute)) ?? null;
}

function firstSelectableDateTime(date: Date) {
  for (const hour of hourOptions) {
    for (const minute of minuteOptions) {
      if (!isDueTimeDisabled(date, hour, minute)) {
        return dateWithTime(date, hour, minute);
      }
    }
  }

  const tomorrow = new Date(date);
  tomorrow.setDate(date.getDate() + 1);
  return dateWithTime(tomorrow, hourOptions[0], minuteOptions[0]);
}

function getDefaultDueDate(now: Date) {
  const defaultToday = dateWithTime(now, DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE);
  if (defaultToday.getTime() >= now.getTime()) return defaultToday;
  return firstSelectableDateTime(now);
}

function resolveDueDate(date: Date, hour: string, minute: string) {
  if (isDueDateDisabled(date)) return null;

  const preferred = dateWithTime(date, hour, minute);
  if (!isDueTimeDisabled(date, hour, minute)) return preferred;

  return firstSelectableDateTime(date);
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
