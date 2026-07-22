import { useEffect } from "react";
import {
  applyTheme,
  emitThemeChange,
  subscribeThemeChanges,
  syncEyeCareWindowBackground,
  watchSystemTheme,
} from "@/lib/theme";
import { dismissBootSplash } from "@/lib/boot";
import { api } from "@/lib/api";
import { EyeCareReminderPage } from "@/pages/EyeCareReminderPage";
import { ShelfPickerPage } from "@/pages/ShelfPickerPage";
import { PomodoroFloatPage } from "@/pages/PomodoroFloatPage";
import { CommandPalettePage } from "@/pages/CommandPalettePage";
import type { Settings } from "@/types";

function App() {
  const view = new URLSearchParams(window.location.search).get("view");
  if (view === "eye-care") {
    return <EyeCareReminderPage />;
  }

  if (view === "command-palette") {
    return <CommandPalettePage />;
  }

  if (view === "pomodoro-float") {
    return <PomodoroFloatPage />;
  }

  if (view === "shelf-picker" || view === "clipboard-picker" || view === "snippet-picker") {
    return <ShelfPickerPage />;
  }

  return <HiddenMainHost />;
}

/** Main window stays hidden; hosts theme sync and dismisses the boot splash only. */
function HiddenMainHost() {
  useEffect(() => {
    let cancelled = false;
    let currentTheme: Settings["theme"] = "system";

    api
      .getSettings()
      .then((s) => {
        if (cancelled) return;
        currentTheme = s.theme;
        applyTheme(currentTheme);
        void syncEyeCareWindowBackground();
      })
      .catch(console.error)
      .finally(() => {
        if (!cancelled) dismissBootSplash();
      });

    const unwatchSystem = watchSystemTheme(
      () => currentTheme,
      () => void emitThemeChange("system")
    );
    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      currentTheme = theme;
      applyTheme(theme);
    });

    return () => {
      cancelled = true;
      unwatchSystem();
      unsubscribeTheme();
    };
  }, []);

  return null;
}

export default App;
