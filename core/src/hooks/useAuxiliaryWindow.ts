import { useEffect } from "react";
import {
  applyTheme,
  applyThemeAsync,
  subscribeThemeChanges,
  watchSystemTheme,
} from "@/lib/theme";
import type { Settings } from "@/types";

export function useAuxiliaryWindowShell(className: string) {
  useEffect(() => {
    document.getElementById("boot-splash")?.remove();

    const previousBodyOverflow = document.body.style.overflow;
    const root = document.documentElement;
    root.classList.add(className);
    document.body.classList.add(className);
    document.body.style.overflow = "hidden";

    let currentTheme: Settings["theme"] = "system";
    applyTheme("system");
    void applyThemeFromSettings().then((theme) => {
      currentTheme = theme;
    });
    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      currentTheme = theme;
      applyTheme(theme);
    });
    const unwatchSystemTheme = watchSystemTheme(
      () => currentTheme,
      () => {
        void applyThemeAsync("system");
      }
    );

    return () => {
      root.classList.remove(className);
      document.body.classList.remove(className);
      document.body.style.overflow = previousBodyOverflow;
      unwatchSystemTheme();
      unsubscribeTheme();
    };
  }, [className]);
}

/** Re-read settings and apply theme — call when an overlay becomes visible. */
export async function refreshAuxiliaryWindowTheme(): Promise<Settings["theme"]> {
  return applyThemeFromSettings();
}

async function applyThemeFromSettings(): Promise<Settings["theme"]> {
  try {
    const { api } = await import("@/lib/api");
    const settings = await api.getSettings();
    await applyThemeAsync(settings.theme);
    return settings.theme;
  } catch {
    await applyThemeAsync("system");
    return "system";
  }
}
