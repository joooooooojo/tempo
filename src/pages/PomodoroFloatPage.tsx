import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Pause, Play, SkipForward, Square, X } from "lucide-react";
import { api } from "@/lib/api";
import { applyTheme, subscribeThemeChanges } from "@/lib/theme";
import { cn, formatClock, isMacTarget, isWindowsTarget } from "@/lib/utils";
import type { PomodoroState, Settings } from "@/types";

const RING_RADIUS = 13;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

const phaseMeta = {
  work: {
    label: "专注",
    dot: "bg-emerald-400",
    ring: "stroke-emerald-400",
    badge: "bg-emerald-500/14 text-emerald-600 dark:text-emerald-300",
    glow: "from-emerald-400/14 via-transparent to-transparent",
    primary: "bg-emerald-500 hover:bg-emerald-600",
    bar: "bg-emerald-400",
  },
  short_break: {
    label: "短休",
    dot: "bg-sky-400",
    ring: "stroke-sky-400",
    badge: "bg-sky-500/14 text-sky-600 dark:text-sky-300",
    glow: "from-sky-400/14 via-transparent to-transparent",
    primary: "bg-sky-500 hover:bg-sky-600",
    bar: "bg-sky-400",
  },
  long_break: {
    label: "长休",
    dot: "bg-violet-400",
    ring: "stroke-violet-400",
    badge: "bg-violet-500/14 text-violet-600 dark:text-violet-300",
    glow: "from-violet-400/14 via-transparent to-transparent",
    primary: "bg-violet-500 hover:bg-violet-600",
    bar: "bg-violet-400",
  },
} as const;

export function PomodoroFloatPage() {
  const [state, setState] = useState<PomodoroState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const savePositionTimer = useRef(0);

  useEffect(() => {
    document.getElementById("boot-splash")?.remove();

    const previousBodyOverflow = document.body.style.overflow;
    const root = document.documentElement;
    const platformClass = isMacTarget
      ? "pomodoro-float-window--mac"
      : isWindowsTarget
        ? "pomodoro-float-window--windows"
        : "pomodoro-float-window--css-shadow";
    root.classList.add("pomodoro-float-window");
    root.classList.add(platformClass);
    document.body.classList.add("pomodoro-float-window");
    document.body.style.overflow = "hidden";
    void applyThemeFromSettings();

    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      applyTheme(theme);
    });

    return () => {
      root.classList.remove(
        "pomodoro-float-window",
        "pomodoro-float-window--mac",
        "pomodoro-float-window--windows",
        "pomodoro-float-window--css-shadow"
      );
      document.body.classList.remove("pomodoro-float-window");
      document.body.style.overflow = previousBodyOverflow;
      unsubscribeTheme();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [nextState, nextSettings] = await Promise.all([
        api.getPomodoroState(),
        api.getSettings(),
      ]);
      if (!cancelled) {
        setState(nextState);
        setSettings(nextSettings);
      }
    };

    void load();
    const unlisten = listen<PomodoroState>("pomodoro-update", (event) => {
      if (!cancelled) setState(event.payload);
    });
    const fallback = window.setInterval(() => void load(), 2000);

    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
      window.clearInterval(fallback);
    };
  }, []);

  useEffect(() => {
    const appWindow = getCurrentWindow();

    void appWindow.onMoved(({ payload: position }) => {
      window.clearTimeout(savePositionTimer.current);
      savePositionTimer.current = window.setTimeout(() => {
        void api.savePomodoroFloatPosition(position.x, position.y);
      }, 300);
    });

    return () => {
      window.clearTimeout(savePositionTimer.current);
    };
  }, []);

  if (!state || !settings) {
    return <div className="pomodoro-float-page" />;
  }

  const meta = phaseMeta[state.phase];
  const isIdle = state.status === "idle";
  const isRunning = state.status === "running";
  const totalSeconds = isIdle
    ? settings.pomodoro_work_minutes * 60
    : state.phase_total_seconds || 1;
  const remainingSeconds = isIdle ? totalSeconds : state.remaining_seconds;
  const progress = isIdle ? 0 : Math.min(Math.max(1 - remainingSeconds / totalSeconds, 0), 1);
  const dashOffset = RING_CIRCUMFERENCE * (1 - progress);
  const phaseLabel = isIdle ? "待开始" : isRunning ? meta.label : "已暂停";
  const detail = state.active_todo_title
    ? state.active_todo_title
    : isIdle
      ? `${settings.pomodoro_work_minutes} 分钟专注`
      : `第 ${cycleLabel(state, settings)} 轮`;

  return (
    <div
      className="pomodoro-float-page"
      onContextMenu={(event) => {
        event.preventDefault();
        void api.popupPomodoroFloatMenu();
      }}
    >
      <div className={cn("pomodoro-float-panel", isIdle && "pomodoro-float-panel--idle")}>
        <div
          className="pomodoro-float-drag-layer"
          aria-hidden="true"
          data-tauri-drag-region
          onMouseDown={(event) => {
            if (event.button !== 0) return;
            void getCurrentWindow().startDragging();
          }}
        />

        <div
          className={cn(
            "pointer-events-none absolute inset-0 z-[1] bg-gradient-to-br opacity-90",
            isIdle ? "from-emerald-400/10 to-transparent" : meta.glow
          )}
        />

        <div className="pomodoro-float-body">
          <div className="pomodoro-float-ring">
            <svg className="h-full w-full -rotate-90" viewBox="0 0 36 36" aria-hidden="true">
              <circle
                cx="18"
                cy="18"
                r={RING_RADIUS}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                className="text-foreground/10"
              />
              <circle
                cx="18"
                cy="18"
                r={RING_RADIUS}
                fill="none"
                strokeWidth="2"
                strokeLinecap="round"
                strokeDasharray={RING_CIRCUMFERENCE}
                strokeDashoffset={dashOffset}
                className={cn("transition-[stroke-dashoffset] duration-1000 ease-linear", meta.ring)}
              />
            </svg>
            <span
              className={cn("pomodoro-float-ring__dot", meta.dot, isRunning && "animate-pulse")}
              aria-hidden="true"
            />
          </div>

          <div className="pomodoro-float-info">
            <div className="pomodoro-float-time">{formatClock(remainingSeconds)}</div>
            <div className="flex min-w-0 items-center gap-1.5">
              <span
                className={cn(
                  "pomodoro-float-badge shrink-0 whitespace-nowrap",
                  isIdle ? "bg-emerald-500/14 text-emerald-600 dark:text-emerald-300" : meta.badge
                )}
              >
                {phaseLabel}
              </span>
              <span className="min-w-0 truncate text-[11px] text-muted-foreground">{detail}</span>
            </div>
          </div>

          <div className="pomodoro-float-actions">
            {isIdle || !isRunning ? (
              <FloatPrimaryButton label="开始" tone={meta.primary} onClick={() => void start(state)}>
                <Play className="h-3.5 w-3.5" />
              </FloatPrimaryButton>
            ) : (
              <FloatPrimaryButton label="暂停" tone={meta.primary} onClick={() => void pause()}>
                <Pause className="h-3.5 w-3.5" />
              </FloatPrimaryButton>
            )}
            {!isIdle && (
              <>
                <FloatIconButton label="跳过" onClick={() => void skip()}>
                  <SkipForward className="h-3.5 w-3.5" />
                </FloatIconButton>
                <FloatIconButton label="停止" onClick={() => void stop()}>
                  <Square className="h-3 w-3" />
                </FloatIconButton>
              </>
            )}
            <FloatIconButton label="关闭" muted onClick={() => void api.hidePomodoroFloat()}>
              <X className="h-3.5 w-3.5" />
            </FloatIconButton>
          </div>
        </div>

        {!isIdle && (
          <div className="pomodoro-float-progress">
            <div
              className={cn("h-full transition-[width] duration-1000 ease-linear", meta.bar)}
              style={{ width: `${progress * 100}%` }}
            />
          </div>
        )}
      </div>
    </div>
  );
}

function cycleLabel(state: PomodoroState, settings: Settings) {
  return `${Math.min(
    state.cycle_count + (state.phase === "work" ? 1 : 0),
    settings.pomodoro_sessions_per_cycle
  )}/${settings.pomodoro_sessions_per_cycle}`;
}

function FloatPrimaryButton({
  label,
  tone,
  onClick,
  children,
}: {
  label: string;
  tone: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={cn("pomodoro-float-btn pomodoro-float-btn--primary text-white", tone)}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      {children}
    </button>
  );
}

function FloatIconButton({
  label,
  onClick,
  children,
  muted = false,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  muted?: boolean;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={cn("pomodoro-float-btn", muted && "pomodoro-float-btn--muted")}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      {children}
    </button>
  );
}

async function start(state: PomodoroState) {
  try {
    await api.startPomodoro(state.active_todo_id);
  } catch (error) {
    console.error(error);
  }
}

async function pause() {
  await api.pausePomodoro();
}

async function stop() {
  await api.stopPomodoro();
}

async function skip() {
  await api.skipPomodoroPhase();
}

async function applyThemeFromSettings() {
  try {
    const settings = await api.getSettings();
    applyTheme(settings.theme);
  } catch {
    applyTheme("system");
  }
}
