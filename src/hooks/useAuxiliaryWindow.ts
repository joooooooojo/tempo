import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isBlurHideSuppressed } from "@/lib/blurHideGuard";
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

export function useShelfBlurClose(openEvent: string, busy = false) {
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let armed = false;
    let armTimer = 0;

    const armBlurClose = () => {
      window.clearTimeout(armTimer);
      armTimer = window.setTimeout(() => {
        armed = true;
      }, 200);
    };

    const unlistenOpen = listen(openEvent, () => {
      armBlurClose();
    });

    let unlistenBlur: (() => void) | undefined;
    void appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused && armed && !busy && !isBlurHideSuppressed()) {
          void appWindow.hide();
        }
      })
      .then((fn) => {
        unlistenBlur = fn;
      });

    armBlurClose();

    return () => {
      window.clearTimeout(armTimer);
      void unlistenOpen.then((fn) => fn());
      unlistenBlur?.();
    };
  }, [openEvent, busy]);
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
