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
}

export type ReminderEvent =
  | { type: "eye_care" }
  | { type: "night" }
  | { type: "app_limit_warn"; app_name: string; percent: number }
  | { type: "app_limit_reached"; app_name: string };
