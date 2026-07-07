import { useEffect, useState, useCallback } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Toaster, toast } from "sonner";
import { AppLayout } from "@/components/layout/AppLayout";
import { OnboardingDialog } from "@/components/OnboardingDialog";
import { ReminderDialog } from "@/components/ReminderDialog";
import { QuitDialog } from "@/components/QuitDialog";
import { HomePage } from "@/pages/HomePage";
import { EyeCareReminderPage } from "@/pages/EyeCareReminderPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { AboutPage } from "@/pages/AboutPage";
import { api } from "@/lib/api";
import type { ReminderEvent, Settings } from "@/types";

const EYE_CARE_REMINDER_LABEL = "eye-care-reminder";

function App() {
  if (new URLSearchParams(window.location.search).get("view") === "eye-care") {
    return <EyeCareReminderPage />;
  }

  return <MainApp />;
}

function MainApp() {
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [reminder, setReminder] = useState<ReminderEvent | null>(null);
  const [showQuit, setShowQuit] = useState(false);

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

      setReminder(e.payload);
    });

    const unlistenQuit = listen("request-quit", () => setShowQuit(true));

    const unlistenToast = listen<{ message: string }>("toast", (e) => {
      toast.info(e.payload.message);
    });

    getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      await getCurrentWindow().hide();
    });

    return () => {
      unlistenReminder.then((fn) => fn());
      unlistenQuit.then((fn) => fn());
      unlistenToast.then((fn) => fn());
    };
  }, [applyTheme]);

  return (
    <>
      <BrowserRouter>
        <Routes>
          <Route element={<AppLayout />}>
            <Route index element={<HomePage />} />
            <Route path="reports" element={<ReportsPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="about" element={<AboutPage />} />
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

      <QuitDialog
        open={showQuit}
        onCancel={() => setShowQuit(false)}
        onConfirm={async () => {
          setShowQuit(false);
          await api.quitApp();
        }}
      />

      <Toaster position="top-center" richColors toastOptions={{ className: "glass rounded-lg" }} />
    </>
  );
}

async function openEyeCareReminderWindow() {
  const existing = await WebviewWindow.getByLabel(EYE_CARE_REMINDER_LABEL);
  if (existing) {
    const isVisible = await existing.isVisible().catch(() => false);
    if (isVisible) {
      await existing.setFocus();
    }
    return;
  }

  const reminderWindow = new WebviewWindow(EYE_CARE_REMINDER_LABEL, {
    url: "/?view=eye-care",
    title: "护眼提醒",
    fullscreen: true,
    decorations: false,
    resizable: false,
    maximizable: false,
    minimizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    focus: false,
    visible: false,
    visibleOnAllWorkspaces: true,
    backgroundColor: "#effbf4",
  });

  reminderWindow.once("tauri://error", (event) => {
    console.error("Failed to create eye-care reminder window", event.payload);
  });
}

export default App;
