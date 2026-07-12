import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyTheme, subscribeThemeChanges } from "@/lib/theme";
import { isMacTarget } from "@/lib/utils";

export function useAuxiliaryWindowShell(className: string) {
  useEffect(() => {
    document.getElementById("boot-splash")?.remove();

    const previousBodyOverflow = document.body.style.overflow;
    const root = document.documentElement;
    root.classList.add(className);
    root.classList.add(isMacTarget ? `${className}--mac` : `${className}--css-shadow`);
    document.body.classList.add(className);
    document.body.style.overflow = "hidden";

    applyTheme("system");
    void applyThemeFromSettings();
    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      applyTheme(theme);
    });

    return () => {
      root.classList.remove(className, `${className}--mac`, `${className}--css-shadow`);
      document.body.classList.remove(className);
      document.body.style.overflow = previousBodyOverflow;
      unsubscribeTheme();
    };
  }, [className]);
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
        if (!focused && armed && !busy) {
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

async function applyThemeFromSettings() {
  try {
    const { api } = await import("@/lib/api");
    const settings = await api.getSettings();
    applyTheme(settings.theme);
  } catch {
    applyTheme("system");
  }
}
