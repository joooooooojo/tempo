import { invoke } from "@tauri-apps/api/core";
import type {
  AppLimit,
  AppUsage,
  DashboardData,
  DailyReport,
  PomodoroState,
  Settings,
  TodoImage,
  TodoItem,
  TodoNote,
  WeeklyReport,
} from "@/types";

export interface TodoImageInput {
  data_url: string;
  mime_type: string;
}

export const api = {
  getDashboard: () => invoke<DashboardData>("get_dashboard"),
  getDailyReport: (date?: string) =>
    invoke<DailyReport>("get_daily_report", { date }),
  getWeeklyReport: () => invoke<WeeklyReport>("get_weekly_report"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Partial<Settings>) =>
    invoke<void>("update_settings", { settings }),
  resetToday: () => invoke<void>("reset_today"),
  resetAll: () => invoke<void>("reset_all"),
  getBlockedApps: () => invoke<string[]>("get_blocked_apps"),
  blockApp: (appName: string) => invoke<void>("block_app", { appName }),
  unblockApp: (appName: string) => invoke<void>("unblock_app", { appName }),
  getAppLimits: () => invoke<AppLimit[]>("get_app_limits"),
  setAppLimit: (appName: string, limitSeconds: number) =>
    invoke<void>("set_app_limit", { appName, limitSeconds }),
  removeAppLimit: (appName: string) =>
    invoke<void>("remove_app_limit", { appName }),
  getTodos: () => invoke<TodoItem[]>("get_todos"),
  addTodo: (title: string, content: string, dueAt?: string | null, images: TodoImageInput[] = []) =>
    invoke<TodoItem>("add_todo", { title, content, dueAt, images }),
  updateTodoTitle: (id: number, title: string) =>
    invoke<TodoItem>("update_todo_title", { id, title }),
  updateTodoDetails: (id: number, title: string, content: string, dueAt?: string | null) =>
    invoke<TodoItem>("update_todo_details", { id, title, content, dueAt }),
  setTodoCompleted: (id: number, completed: boolean) =>
    invoke<TodoItem>("set_todo_completed", { id, completed }),
  setTodoPinned: (id: number, pinned: boolean) =>
    invoke<TodoItem>("set_todo_pinned", { id, pinned }),
  addTodoImage: (todoId: number, image: TodoImageInput) =>
    invoke<TodoItem>("add_todo_image", { todoId, image }),
  deleteTodoImage: (imageId: TodoImage["id"]) =>
    invoke<TodoItem>("delete_todo_image", { imageId }),
  addTodoNote: (todoId: number, body: string, images: TodoImageInput[] = []) =>
    invoke<TodoItem>("add_todo_note", { todoId, body, images }),
  deleteTodoNote: (noteId: number) =>
    invoke<TodoItem>("delete_todo_note", { noteId }),
  restoreTodoNote: (note: TodoNote) =>
    invoke<TodoItem>("restore_todo_note", { note }),
  deleteTodo: (id: number) => invoke<void>("delete_todo", { id }),
  restoreTodo: (todo: TodoItem) => invoke<TodoItem>("restore_todo", { todo }),
  clearCompletedTodos: () => invoke<number>("clear_completed_todos"),
  getKnownApps: () => invoke<AppUsage[]>("get_known_apps"),
  exportReport: (path: string) => invoke<void>("export_report", { path }),
  exportTodosBackup: (path: string) =>
    invoke<void>("export_todos_backup", { path }),
  importTodosBackup: (path: string) =>
    invoke<TodoItem[]>("import_todos_backup", { path }),
  saveMarkdownImage: (dataUrl: string, mimeType: string) =>
    invoke<string>("save_markdown_image", { dataUrl, mimeType }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  quitApp: () => invoke<void>("quit_app"),
  hideToTray: () => invoke<void>("hide_to_tray_command"),
  showWindow: () => invoke<void>("show_window"),
  getPomodoroState: () => invoke<PomodoroState>("get_pomodoro_state"),
  startPomodoro: () => invoke<PomodoroState>("start_pomodoro"),
  pausePomodoro: () => invoke<PomodoroState>("pause_pomodoro"),
  stopPomodoro: () => invoke<PomodoroState>("stop_pomodoro"),
  skipPomodoroPhase: () => invoke<PomodoroState>("skip_pomodoro_phase"),
};
