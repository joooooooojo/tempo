import { useEffect, useState, useCallback } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Toaster, toast } from "sonner";
import { AppLayout } from "@/components/layout/AppLayout";
import { OnboardingDialog } from "@/components/OnboardingDialog";
import { ReminderDialog } from "@/components/ReminderDialog";
import { EyeCareReminderPage } from "@/pages/EyeCareReminderPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { PomodoroPage } from "@/pages/PomodoroPage";
import { TodoPage } from "@/pages/TodoPage";
import { QuickTodoPage } from "@/pages/QuickTodoPage";
import { api } from "@/lib/api";
import { playNotificationSound } from "@/lib/sound";
import { appToastOptions } from "@/lib/toastOptions";
import type { ReminderEvent, Settings } from "@/types";

function App() {
  const view = new URLSearchParams(window.location.search).get("view");
  if (view === "eye-care") {
    return <EyeCareReminderPage />;
  }

  if (view === "quick-todo") {
    return <QuickTodoPage />;
  }

  return <MainApp />;
}

function MainApp() {
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [reminder, setReminder] = useState<ReminderEvent | null>(null);

  const applyTheme = useCallback((theme: Settings["theme"]) => {
    const root = document.documentElement;
    if (theme === "dark") root.classList.add("dark");
    else if (theme === "light") root.classList.remove("dark");
    else {
      root.classList.toggle(
        "dark",
        window.matchMedia("(prefers-color-scheme: dark)").matches
      );
    }
  }, []);

  useEffect(() => {
    api.getSettings().then((s) => {
      applyTheme(s.theme);
      if (!s.onboarding_completed) setShowOnboarding(true);
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

      setReminder(e.payload);
    });

    const unlistenToast = listen<{ message: string }>("toast", (e) => {
      toast.info(e.payload.message);
    });

    return () => {
      unlistenReminder.then((fn) => fn());
      unlistenToast.then((fn) => fn());
    };
  }, [applyTheme]);

  return (
    <>
      <BrowserRouter>
        <Routes>
          <Route element={<AppLayout />}>
            <Route index element={<TodoPage />} />
            <Route path="pomodoro" element={<PomodoroPage />} />
            <Route path="reports" element={<ReportsPage />} />
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
