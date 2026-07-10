import type { ClipboardEvent, Dispatch, SetStateAction } from "react";
import { toast } from "sonner";
import { api, type TodoImageInput } from "@/lib/api";
import type { TodoItem } from "@/types";
import type { DraftTodoImage } from "@/components/todos/TodoCreateDialog";

export type TodoFilter = "active" | "completed";

export type DraftImage = DraftTodoImage;

export const MAX_IMAGES_PER_NOTE = 4;
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export const TODO_PAGE_SIZE = 10;

export function filterCount(filter: TodoFilter, todos: TodoItem[]) {
  if (filter === "active") return todos.filter((todo) => !todo.completed).length;
  return todos.filter((todo) => todo.completed).length;
}

export function emptyText(filter: TodoFilter, searching = false) {
  if (searching) return "没有匹配的待办";
  if (filter === "completed") return "暂无已完成事项";
  return "暂无未完成事项";
}

export function replaceTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos(items.map((item) => (item.id === todo.id ? todo : item)));
}

export function upsertTodo(items: TodoItem[], todo: TodoItem) {
  return sortTodos([todo, ...items.filter((item) => item.id !== todo.id)]);
}

export function sortTodos(items: TodoItem[]) {
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

export function todoSummary(todo: TodoItem) {
  const cleaned = plainTextSummary(todo.content);
  if (!cleaned || cleaned === todo.title.trim()) return "";
  return cleaned;
}

export function normalizeTodo(todo: TodoItem): TodoItem {
  const images = todo.images ?? [];
  return {
    ...todo,
    recurrence: todo.recurrence ?? "none",
    remind_1d: todo.remind_1d ?? false,
    remind_1h: todo.remind_1h ?? false,
    remind_custom_hours: todo.remind_custom_hours ?? null,
    subtasks: todo.subtasks ?? [],
    tags: todo.tags ?? [],
    images,
    notes: todo.notes ?? [],
    image_count: todo.image_count ?? images.length,
    lightweight: todo.lightweight ?? false,
  };
}

export function todoImageCount(todo: TodoItem): number {
  if (todo.images.length > 0) return todo.images.length;
  return todo.image_count ?? 0;
}

export function matchesTodoSearch(todo: TodoItem, query: string) {
  return [
    todo.title,
    todo.content,
    ...todo.notes.map((note) => note.body),
    ...todo.subtasks.map((subtask) => subtask.title),
    ...todo.tags,
  ].some((value) => normalizeSearch(value).includes(query));
}

export function normalizeSearch(value: string) {
  return value.trim().toLocaleLowerCase();
}

export function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function plainTextSummary(value: string) {
  return value
    .replace(/!\[[^\]]*]\([^)]*\)/g, "图片")
    .replace(/\[([^\]]+)]\([^)]*\)/g, "$1")
    .replace(/[`*_~>#-]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 120);
}

export function toTimestamp(value?: string | null) {
  if (!value) return Number.POSITIVE_INFINITY;
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? Number.POSITIVE_INFINITY : time;
}

export function formatTodoDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function toDateTimeLocalValue(value?: string | Date | null) {
  if (!value) return "";
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";

  const offset = date.getTimezoneOffset();
  const localDate = new Date(date.getTime() - offset * 60_000);
  return localDate.toISOString().slice(0, 16);
}

export function toIsoDateTime(value: string) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

export function isOverdue(value?: string | null) {
  return Boolean(value && new Date(value).getTime() < Date.now());
}

export function isDueSoon(value?: string | null) {
  if (!value) return false;
  const time = new Date(value).getTime();
  if (Number.isNaN(time)) return false;
  const now = Date.now();
  return time >= now && time - now <= 24 * 60 * 60 * 1000;
}

export function dueBadgeClass(todo: TodoItem) {
  if (todo.completed) return "bg-foreground/5 text-muted-foreground";
  if (isOverdue(todo.due_at)) return "bg-rose-500/12 text-rose-600 dark:text-rose-300";
  if (isDueSoon(todo.due_at)) return "bg-amber-400/16 text-amber-700 dark:text-amber-300";
  return "bg-primary/10 text-primary";
}

export function readFileAsDataUrl(file: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("图片读取失败"));
    reader.readAsDataURL(file);
  });
}

export function toTodoImageInput(image: DraftImage): TodoImageInput {
  return {
    data_url: image.data_url,
    mime_type: image.mime_type,
  };
}

export function createLocalId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败";
}

export async function ensureTodoDetails(
  todoId: number,
  setTodos: Dispatch<SetStateAction<TodoItem[]>>,
) {
  let needsHydration = false;
  setTodos((current) => {
    const todo = current.find((item) => item.id === todoId);
    needsHydration = Boolean(todo?.lightweight);
    return current;
  });
  if (!needsHydration) return;

  try {
    const full = await api.getTodo(todoId);
    setTodos((current) => replaceTodo(current, normalizeTodo(full)));
  } catch (error) {
    console.error(error);
  }
}

export async function imagesFromClipboard(event: ClipboardEvent<HTMLElement>) {
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
