import type { TodoRecurrence } from "@/types";

export const recurrenceOptions: Array<{ value: TodoRecurrence; label: string }> = [
  { value: "none", label: "不重复" },
  { value: "daily", label: "每天" },
  { value: "weekly", label: "每周" },
  { value: "monthly", label: "每月" },
];

export function recurrenceLabel(value: TodoRecurrence) {
  return recurrenceOptions.find((option) => option.value === value)?.label ?? "不重复";
}

export function todoReminderLabel(todo: {
  remind_1d: boolean;
  remind_1h: boolean;
  remind_custom_hours?: number | null;
}) {
  const parts: string[] = [];
  if (todo.remind_1d) parts.push("提前 1 天");
  if (todo.remind_1h) parts.push("提前 1 小时");
  if (todo.remind_custom_hours) parts.push(`提前 ${todo.remind_custom_hours} 小时`);
  if (parts.length === 0) return null;
  return `${parts.join(" / ")}提醒`;
}

export function subtaskProgress(subtasks: Array<{ completed: boolean }>) {
  if (subtasks.length === 0) return null;
  const completed = subtasks.filter((item) => item.completed).length;
  return `${completed}/${subtasks.length}`;
}
