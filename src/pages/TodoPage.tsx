import { useEffect, useMemo, useState, type FormEvent, type KeyboardEvent } from "react";
import { toast } from "sonner";
import {
  Check,
  CheckCircle2,
  Circle,
  ClipboardList,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { TodoItem } from "@/types";

type TodoFilter = "all" | "active" | "completed";

const filters: Array<{ value: TodoFilter; label: string }> = [
  { value: "active", label: "未完成" },
  { value: "all", label: "全部" },
  { value: "completed", label: "已完成" },
];

export function TodoPage() {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [title, setTitle] = useState("");
  const [filter, setFilter] = useState<TodoFilter>("active");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");

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

  const activeCount = todos.filter((todo) => !todo.completed).length;
  const completedCount = todos.length - activeCount;
  const completionRate = todos.length === 0 ? 0 : Math.round((completedCount / todos.length) * 100);

  const visibleTodos = useMemo(() => {
    if (filter === "active") return todos.filter((todo) => !todo.completed);
    if (filter === "completed") return todos.filter((todo) => todo.completed);
    return todos;
  }, [filter, todos]);

  const handleAdd = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!title.trim()) {
      toast.error("请输入待办内容");
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(title);
      setTodos((current) => sortTodos([created, ...current]));
      setTitle("");
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
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditTitle("");
  };

  const commitEdit = async (todo: TodoItem) => {
    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      toast.error("请输入待办内容");
      return;
    }

    if (nextTitle === todo.title) {
      cancelEdit();
      return;
    }

    try {
      const updated = await api.updateTodoTitle(todo.id, nextTitle);
      setTodos((current) => replaceTodo(current, updated));
      cancelEdit();
      toast.success("已更新");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const handleEditKey = (event: KeyboardEvent<HTMLInputElement>, todo: TodoItem) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void commitEdit(todo);
    }

    if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
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

  const clearCompleted = async () => {
    if (completedCount === 0) return;

    try {
      const deleted = await api.clearCompletedTodos();
      setTodos((current) => current.filter((todo) => !todo.completed));
      toast.success(deleted > 0 ? `已清理 ${deleted} 项` : "没有已完成事项");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div className="flex items-end justify-between gap-4">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
            待办
          </p>
          <h1 className="mt-1 text-2xl font-extrabold tracking-tight">待办事项</h1>
        </div>
        <Badge variant={activeCount > 0 ? "default" : "secondary"} className="h-7 px-3">
          {activeCount > 0 ? `${activeCount} 项待完成` : "全部完成"}
        </Badge>
      </div>

      <div className="grid grid-cols-3 gap-3">
        <TodoStat label="未完成" value={activeCount} />
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

      <Card>
        <CardContent className="p-4">
          <form className="flex gap-2" onSubmit={handleAdd}>
            <Input
              value={title}
              maxLength={120}
              placeholder="添加新的待办事项"
              onChange={(event) => setTitle(event.target.value)}
            />
            <Button className="shrink-0" disabled={saving}>
              <Plus className="h-4 w-4" />
              添加
            </Button>
          </form>
        </CardContent>
      </Card>

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
          variant="outline"
          size="sm"
          className="h-9"
          disabled={completedCount === 0}
          onClick={clearCompleted}
        >
          <Trash2 className="h-3.5 w-3.5" />
          清空已完成
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
                const isEditing = editingId === todo.id;

                return (
                  <div
                    key={todo.id}
                    className={cn(
                      "grid grid-cols-[36px_minmax(0,1fr)_auto] items-center gap-3 px-4 py-3.5 transition-colors hover:bg-foreground/[0.03]",
                      todo.completed && "bg-foreground/[0.018]"
                    )}
                  >
                    <button
                      type="button"
                      className="flex h-9 w-9 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-primary"
                      aria-label={todo.completed ? "恢复待办" : "完成待办"}
                      onClick={() => void toggleTodo(todo)}
                    >
                      {todo.completed ? (
                        <CheckCircle2 className="h-5 w-5 text-primary" />
                      ) : (
                        <Circle className="h-5 w-5" />
                      )}
                    </button>

                    {isEditing ? (
                      <form
                        className="flex min-w-0 items-center gap-2"
                        onSubmit={(event) => {
                          event.preventDefault();
                          void commitEdit(todo);
                        }}
                      >
                        <Input
                          autoFocus
                          value={editTitle}
                          maxLength={120}
                          className="h-9"
                          onChange={(event) => setEditTitle(event.target.value)}
                          onKeyDown={(event) => handleEditKey(event, todo)}
                        />
                        <Button type="submit" size="icon" className="h-9 w-9 shrink-0" aria-label="保存">
                          <Check className="h-4 w-4" />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9 shrink-0"
                          aria-label="取消"
                          onClick={cancelEdit}
                        >
                          <X className="h-4 w-4" />
                        </Button>
                      </form>
                    ) : (
                      <button
                        type="button"
                        className="min-w-0 text-left"
                        onClick={() => void toggleTodo(todo)}
                      >
                        <p
                          className={cn(
                            "truncate text-[14px] font-semibold transition-colors",
                            todo.completed && "text-muted-foreground line-through"
                          )}
                        >
                          {todo.title}
                        </p>
                        <p className="mt-1 text-[11px] text-muted-foreground">
                          {todo.completed && todo.completed_at
                            ? `完成于 ${formatTodoDate(todo.completed_at)}`
                            : `创建于 ${formatTodoDate(todo.created_at)}`}
                        </p>
                      </button>
                    )}

                    <div className="flex items-center gap-1">
                      {!isEditing && (
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8 text-muted-foreground"
                          aria-label="编辑"
                          onClick={() => startEdit(todo)}
                        >
                          <Pencil className="h-3.5 w-3.5" />
                        </Button>
                      )}
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

function TodoStat({ label, value }: { label: string; value: number }) {
  return (
    <Card>
      <CardContent className="p-3.5">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </p>
        <p className="stat-value mt-1 text-2xl font-bold text-primary">{value}</p>
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

function filterCount(filter: TodoFilter, todos: TodoItem[]) {
  if (filter === "active") return todos.filter((todo) => !todo.completed).length;
  if (filter === "completed") return todos.filter((todo) => todo.completed).length;
  return todos.length;
}

function emptyText(filter: TodoFilter) {
  if (filter === "completed") return "暂无已完成事项";
  if (filter === "all") return "暂无待办事项";
  return "暂无未完成事项";
}

function replaceTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos(items.map((item) => (item.id === todo.id ? todo : item)));
}

function sortTodos(items: TodoItem[]) {
  return [...items].sort((a, b) => {
    if (a.completed !== b.completed) return Number(a.completed) - Number(b.completed);
    const aTime = new Date(a.completed_at ?? a.created_at).getTime();
    const bTime = new Date(b.completed_at ?? b.created_at).getTime();
    return bTime - aTime || b.id - a.id;
  });
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

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败";
}
