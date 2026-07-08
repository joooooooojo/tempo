import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Eye } from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { Settings } from "@/types";

export function EyeCareReminderPage() {
  const rootRef = useRef<HTMLDivElement>(null);
  const hidingRef = useRef(false);
  const [revealed, setRevealed] = useState(false);

  const revealOverlay = useCallback(() => {
    setRevealed(true);
    window.requestAnimationFrame(() => {
      rootRef.current?.focus();
    });
  }, []);

  const hideOverlay = useCallback(() => {
    if (hidingRef.current) return;
    hidingRef.current = true;
    setRevealed(false);

    invoke("hide_eye_care_overlay")
      .catch((error) => {
        console.error("Failed to hide eye-care overlay", error);
      })
      .finally(() => {
        hidingRef.current = false;
      });
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.add("eye-care-window");
    document.body.classList.add("eye-care-window");
    void applyThemeFromSettings();

    const unlistenReveal = listen("eye-care:reveal", () => {
      setRevealed(false);
      window.requestAnimationFrame(() => {
        revealOverlay();
      });
    });

    void getCurrentWindow()
      .isVisible()
      .then((visible) => {
        if (visible) revealOverlay();
      });

    const handleKeyDown = () => hideOverlay();
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      root.classList.remove("eye-care-window");
      document.body.classList.remove("eye-care-window");
      void unlistenReveal.then((fn) => fn());
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [hideOverlay, revealOverlay]);

  return (
    <main
      ref={rootRef}
      tabIndex={0}
      role="dialog"
      aria-label="护眼提醒"
      className={cn(
        "eye-care-overlay flex h-screen w-screen cursor-pointer items-center justify-center overflow-hidden bg-[radial-gradient(circle_at_center,hsl(143_58%_96%),hsl(154_40%_90%))] px-8 text-emerald-950 outline-none transition-[filter] duration-300 dark:bg-[radial-gradient(circle_at_center,hsl(164_22%_15%),hsl(160_25%_8%))] dark:text-emerald-50",
        revealed ? "is-revealed" : "brightness-[0.96]"
      )}
      onPointerDown={hideOverlay}
    >
      <section className="eye-care-content pointer-events-none flex max-w-xl flex-col items-center text-center">
        <div className="eye-care-icon flex h-[72px] w-[72px] items-center justify-center rounded-xl bg-white/70 text-emerald-600 shadow-[0_18px_45px_rgba(16,185,129,0.18)] ring-1 ring-emerald-900/5 dark:bg-white/8 dark:text-emerald-200 dark:ring-white/10">
          <Eye className="h-9 w-9" strokeWidth={1.8} />
        </div>

        <h1 className="eye-care-title mt-8 text-[34px] font-semibold text-emerald-950/82 dark:text-emerald-50">
          休息一下眼睛
        </h1>

        <p className="eye-care-copy mt-4 max-w-lg text-[16px] leading-7 text-emerald-950/58 dark:text-emerald-50/62">
          看向远处，放松肩颈，让眼睛离开屏幕一会儿。
        </p>

        <p className="eye-care-hint mt-10 rounded-lg bg-white/62 px-4 py-2 text-[13px] font-medium text-emerald-950/48 shadow-sm ring-1 ring-emerald-900/5 dark:bg-white/8 dark:text-emerald-50/50 dark:ring-white/10">
          按任意键或点击屏幕关闭
        </p>
      </section>
    </main>
  );
}

async function applyThemeFromSettings() {
  try {
    const settings = await api.getSettings();
    applyTheme(settings.theme);
  } catch {
    applyTheme("system");
  }
}

function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;

  if (theme === "dark") {
    root.classList.add("dark");
    return;
  }

  if (theme === "light") {
    root.classList.remove("dark");
    return;
  }

  root.classList.toggle("dark", window.matchMedia("(prefers-color-scheme: dark)").matches);
}
