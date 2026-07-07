import { invoke } from "@tauri-apps/api/core";
import type {
  AppLimit,
  AppUsage,
  DashboardData,
  DailyReport,
  Settings,
  WeeklyReport,
} from "@/types";

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
  getKnownApps: () => invoke<AppUsage[]>("get_known_apps"),
  exportReport: (path: string) => invoke<void>("export_report", { path }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  quitApp: () => invoke<void>("quit_app"),
  showWindow: () => invoke<void>("show_window"),
};
