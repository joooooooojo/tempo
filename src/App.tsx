import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Toaster, toast } from "sonner";
import { AppLayout } from "@/components/layout/AppLayout";
import { OnboardingDialog } from "@/components/OnboardingDialog";
import { ReminderDialog } from "@/components/ReminderDialog";
import { EyeCareReminderPage } from "@/pages/EyeCareReminderPage";
import { ClipboardPage } from "@/pages/ClipboardPage";
import { ShelfPickerPage } from "@/pages/ShelfPickerPage";
import { PomodoroFloatPage } from "@/pages/PomodoroFloatPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { PomodoroPage } from "@/pages/PomodoroPage";
import { SnippetsPage } from "@/pages/SnippetsPage";
import { TodoPage } from "@/pages/TodoPage";
import { QuickTodoPage } from "@/pages/QuickTodoPage";
import { api } from "@/lib/api";
import { revealAppShell } from "@/lib/boot";
import { notifyUser } from "@/lib/notifications";
import { playNotificationSound } from "@/lib/sound";
import { appToastOptions } from "@/lib/toastOptions";
import {
  applyTheme,
  emitThemeChange,
  subscribeThemeChanges,
  syncEyeCareWindowBackground,
  watchSystemTheme,
} from "@/lib/theme";
import type { ReminderEvent, Settings } from "@/types";

function App() {
  const view = new URLSearchParams(window.location.search).get("view");
  if (view === "eye-care") {
    return <EyeCareReminderPage />;
  }

  if (view === "quick-todo") {
    return <QuickTodoPage />;
  }

  if (view === "pomodoro-float") {
    return <PomodoroFloatPage />;
  }

  if (view === "shelf-picker" || view === "clipboard-picker" || view === "snippet-picker") {
    return <ShelfPickerPage />;
  }

  return <MainApp />;
}

function MainApp() {
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [reminder, setReminder] = useState<ReminderEvent | null>(null);

  useEffect(() => {
    let cancelled = false;
    let currentTheme: Settings["theme"] = "system";
    const safetyTimer = window.setTimeout(() => {
      if (!cancelled) void revealAppShell();
    }, 8000);

    api
      .getSettings()
      .then((s) => {
        if (cancelled) return;
        currentTheme = s.theme;
        applyTheme(currentTheme);
        void syncEyeCareWindowBackground();
        if (!s.onboarding_completed) setShowOnboarding(true);
      })
      .catch(console.error)
      .finally(() => {
        if (!cancelled) void revealAppShell();
      });

    const unwatchSystem = watchSystemTheme(
      () => currentTheme,
      () => void emitThemeChange("system")
    );
    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      currentTheme = theme;
      applyTheme(theme);
    });

    const unlistenReminder = listen<ReminderEvent>("reminder", (e) => {
      if (e.payload.type === "eye_care") {
        void openEyeCareReminderWindow();
        return;
      }

      if (e.payload.type === "pomodoro_phase_end") {
        api.getSettings().then((s) => {
          if (s.sound_enabled) playNotificationSound();
        });
      }

      if (e.payload.type === "todo_due") {
        api.getSettings().then((s) => {
          if (s.sound_enabled) playNotificationSound();
        });
        const leadText =
          e.payload.lead === "1d"
            ? "将在 1 天后截止"
            : e.payload.lead === "1h"
              ? "将在 1 小时后截止"
              : "已到截止时间";
        void notifyUser("待办提醒", `「${e.payload.title}」${leadText}`);
      }

      setReminder(e.payload);
    });

    const unlistenToast = listen<{ message: string }>("toast", (e) => {
      toast.info(e.payload.message);
    });

    return () => {
      cancelled = true;
      window.clearTimeout(safetyTimer);
      unwatchSystem();
      unsubscribeTheme();
      unlistenReminder.then((fn) => fn());
      unlistenToast.then((fn) => fn());
    };
  }, []);

  return (
    <>
      <BrowserRouter>
        <Routes>
          <Route element={<AppLayout />}>
            <Route index element={<TodoPage />} />
            <Route path="pomodoro" element={<PomodoroPage />} />
            <Route path="reports" element={<ReportsPage />} />
            <Route path="clipboard" element={<ClipboardPage />} />
            <Route path="snippets" element={<SnippetsPage />} />
            <Route path="settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </BrowserRouter>

      <OnboardingDialog
        open={showOnboarding}
        onComplete={async () => {
          await api.completeOnboarding();
          setShowOnboarding(false);
        }}
      />

      <ReminderDialog event={reminder} onDismiss={() => setReminder(null)} />

      <Toaster position="top-center" richColors toastOptions={appToastOptions} />
    </>
  );
}

async function openEyeCareReminderWindow() {
  try {
    await invoke("show_eye_care_overlay");
  } catch (error) {
    console.error("Failed to open eye-care overlay", error);
  }
}

export default App;
