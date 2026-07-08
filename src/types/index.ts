export interface AppUsage {
  app_name: string;
  process_name: string;
  category: string;
  seconds: number;
  icon_data_url?: string | null;
}

export interface DashboardData {
  today_screen_seconds: number;
  week_screen_seconds: number;
  month_screen_seconds: number;
  top_apps: AppUsage[];
  continuous_screen_seconds: number;
  status_message: string;
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
}

export interface AppLimit {
  app_name: string;
  limit_seconds: number;
  used_seconds: number;
  warn_sent: boolean;
  limit_sent: boolean;
}

export interface TodoItem {
  id: number;
  title: string;
  completed: boolean;
  due_at?: string | null;
  created_at: string;
  completed_at?: string | null;
  images: TodoImage[];
  notes: TodoNote[];
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
}

export interface PomodoroState {
  status: "idle" | "running" | "paused";
  phase: "work" | "short_break" | "long_break";
  remaining_seconds: number;
  phase_total_seconds: number;
  sessions_today: number;
  cycle_count: number;
}

export type ReminderEvent =
  | { type: "eye_care" }
  | { type: "night" }
  | { type: "app_limit_warn"; app_name: string; percent: number }
  | { type: "app_limit_reached"; app_name: string }
  | { type: "pomodoro_phase_end"; phase: "work" | "short_break" | "long_break"; skipped: boolean };
