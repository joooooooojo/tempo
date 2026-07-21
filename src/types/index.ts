export interface AppUsage {
  app_name: string;
  process_name: string;
  category: string;
  seconds: number;
  icon_data_url?: string | null;
}

export interface HourlyData {
  hour: number;
  seconds: number;
}

export interface DailyReport {
  date: string;
  total_seconds: number;
  average_seconds: number;
  peak_hour: number;
  peak_seconds: number;
  hourly: HourlyData[];
  top_apps: AppUsage[];
}

export interface WeeklyDay {
  date: string;
  seconds: number;
  is_over_limit: boolean;
}

export interface WeeklyReport {
  days: WeeklyDay[];
  average_seconds: number;
  daily_limit_seconds: number;
  top_apps: AppUsage[];
}

export interface TodoSubtask {
  id: number;
  todo_id: number;
  title: string;
  completed: boolean;
  sort_order: number;
  created_at: string;
}

export type TodoRecurrence = "none" | "daily" | "weekly" | "monthly";

export interface TodoItem {
  id: number;
  title: string;
  content: string;
  completed: boolean;
  due_at?: string | null;
  pinned_at?: string | null;
  created_at: string;
  completed_at?: string | null;
  recurrence: TodoRecurrence;
  remind_1d: boolean;
  remind_1h: boolean;
  remind_custom_hours?: number | null;
  recurrence_root_id?: number | null;
  next_recurrence_at?: string | null;
  images: TodoImage[];
  notes: TodoNote[];
  subtasks: TodoSubtask[];
  tags: string[];
  image_count?: number;
  lightweight?: boolean;
}

export interface TodoImage {
  id: number;
  todo_id: number;
  data_url: string;
  mime_type: string;
  created_at: string;
}

export interface TodoNote {
  id: number;
  todo_id: number;
  body: string;
  created_at: string;
  images: TodoNoteImage[];
}

export interface TodoNoteImage {
  id: number;
  note_id: number;
  data_url: string;
  mime_type: string;
  created_at: string;
}

export interface Settings {
  autostart: boolean;
  sound_enabled: boolean;
  theme: "light" | "dark" | "system";
  eye_care_enabled: boolean;
  eye_care_interval_minutes: number;
  night_reminder_enabled: boolean;
  night_reminder_start: string;
  night_reminder_end: string;
  onboarding_completed: boolean;
  pomodoro_work_minutes: number;
  pomodoro_short_break_minutes: number;
  pomodoro_long_break_minutes: number;
  pomodoro_sessions_per_cycle: number;
  pomodoro_float_enabled: boolean;
  pomodoro_float_auto_show: boolean;
  clipboard_monitor_enabled: boolean;
  clipboard_max_entries: number;
  clipboard_paste_mode: "clipboard" | "active_app";
  clipboard_plain_text_only: boolean;
  clipboard_history_retention: "days" | "weeks" | "months" | "years" | "permanent";
  shortcut_quick_todo: string;
  shortcut_clipboard_picker: string;
  shortcut_snippet_picker: string;
  storage_dir: string;
  mcp_enabled: boolean;
  mcp_port: number;
  mcp_token: string;
}

export interface ClipboardEntry {
  id: number;
  content: string;
  kind: "text" | "image" | string;
  source_app?: string | null;
  source_process?: string | null;
  source_icon_data_url?: string | null;
  image_width?: number | null;
  image_height?: number | null;
  pinned: boolean;
  created_at: string;
}

export interface ClipboardHistoryPage {
  entries: ClipboardEntry[];
  total: number;
  has_more: boolean;
}

export interface Snippet {
  id: number;
  title: string;
  content: string;
  tags: string[];
  group_id?: number | null;
  group_name?: string | null;
  shortcut?: string | null;
  language?: string | null;
  pinned: boolean;
  use_count: number;
  last_used_at?: string | null;
  archived_at?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface SnippetGroup {
  id: number;
  name: string;
  color: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface PomodoroState {
  status: "idle" | "running" | "paused";
  phase: "work" | "short_break" | "long_break";
  remaining_seconds: number;
  phase_total_seconds: number;
  sessions_today: number;
  cycle_count: number;
  active_todo_id: number | null;
  active_todo_title: string | null;
}

export interface TodoFocusSummary {
  todo_id: number;
  sessions_today: number;
  total_seconds_today: number;
  total_seconds_all: number;
  sessions_all: number;
  last_focused_at: string | null;
}

export type ReminderEvent =
  | { type: "eye_care" }
  | { type: "night" }
  | { type: "pomodoro_phase_end"; phase: "work" | "short_break" | "long_break"; skipped: boolean }
  | { type: "todo_due"; todo_id: number; title: string; lead: "1d" | "1h" | "due" | "custom"; hours?: number };

export interface HostsWorkspace {
  path: string;
  writable: boolean;
  authorized: boolean;
  managed: boolean;
  publicContent: string;
  activeProfileId?: string | null;
  profiles: HostsProfile[];
  systemContent: string;
}

export interface HostsProfile {
  id: string;
  name: string;
  updatedAt: string;
  active: boolean;
}

export interface HostsBackup {
  id: string;
  source: string;
  createdAt: string;
  preview: string;
}

export interface PortRecord {
  protocol: "TCP" | "UDP";
  localAddress: string;
  localPort: number;
  remoteAddress?: string | null;
  remotePort?: number | null;
  state: string;
  pid?: number | null;
  processName: string;
  processPath?: string | null;
  processStartedAt?: number | null;
  canTerminate: boolean;
  protectedReason?: string | null;
}

export interface TerminatePortProcessRequest {
  protocol: PortRecord["protocol"];
  localAddress: string;
  localPort: number;
  pid: number;
  processStartedAt: number;
}

export type TranslateProviderId = "youdao" | "baidu" | "tencent" | "google" | "deepl";

export interface TranslateProviderCreds {
  enabled: boolean;
  fields: Record<string, string>;
}

export interface TranslateConfig {
  defaultProvider: string;
  defaultSourceLang: string;
  defaultTargetLang: string;
  compareMode: boolean;
  providers: Record<string, TranslateProviderCreds>;
}

export interface TranslateResult {
  provider: string;
  text: string;
  detectedFrom?: string | null;
  error?: string | null;
}
