import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Pause, Play, RotateCcw, SkipForward, Timer } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { cn, formatClock } from "@/lib/utils";
import type { PomodoroState, Settings } from "@/types";

const phaseMeta = {
  work: {
    label: "专注",
    hint: "保持专注，完成当前任务",
    ring: "stroke-emerald-400",
    glow: "from-emerald-400/20 to-teal-500/10",
    badge: "bg-emerald-500/15 text-emerald-400",
  },
  short_break: {
    label: "短休",
    hint: "站起来活动，放松眼睛",
    ring: "stroke-sky-400",
    glow: "from-sky-400/20 to-blue-500/10",
    badge: "bg-sky-500/15 text-sky-400",
  },
  long_break: {
    label: "长休",
    hint: "好好休息一下，恢复精力",
    ring: "stroke-violet-400",
    glow: "from-violet-400/20 to-purple-500/10",
    badge: "bg-violet-500/15 text-violet-400",
  },
} as const;

export function PomodoroPage() {
  const [state, setState] = useState<PomodoroState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);

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

  if (!state || !settings) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="glass rounded-lg px-8 py-5 text-sm text-muted-foreground">
          加载中...
        </div>
      </div>
    );
  }

  const meta = phaseMeta[state.phase];
  const isIdle = state.status === "idle";
  const isRunning = state.status === "running";
  const totalSeconds = isIdle
    ? settings.pomodoro_work_minutes * 60
    : state.phase_total_seconds || 1;
  const remainingSeconds = isIdle ? totalSeconds : state.remaining_seconds;
  const progress = Math.min(
    Math.max(1 - remainingSeconds / totalSeconds, 0),
    1
  );
  const radius = 108;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - progress);

  const handleStart = async () => {
    setState(await api.startPomodoro());
  };

  const handlePause = async () => {
    setState(await api.pausePomodoro());
  };

  const handleStop = async () => {
    setState(await api.stopPomodoro());
  };

  const handleSkip = async () => {
    setState(await api.skipPomodoroPhase());
  };

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col justify-center space-y-5">
      <div className="flex items-end justify-between">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
            番茄时钟
          </p>
          <h1 className="mt-1 text-2xl font-extrabold tracking-tight">
            {isIdle ? "准备开始专注" : meta.label}
          </h1>
          <p className="mt-1 text-[13px] text-muted-foreground">
            {isIdle ? "按你的节奏，一次专注一件事" : meta.hint}
          </p>
        </div>
        <div className="glass-subtle rounded-lg px-4 py-3 text-right">
          <p className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            今日完成
          </p>
          <p className="mt-0.5 text-2xl font-bold tabular-nums text-primary">
            {state.sessions_today}
          </p>
        </div>
      </div>

      <Card className="glass-glow overflow-hidden">
        <CardContent className="relative p-8">
          <div
            className={cn(
              "pointer-events-none absolute inset-0 bg-gradient-to-br opacity-60",
              isIdle ? "from-emerald-400/10 to-teal-500/5" : meta.glow
            )}
          />

          <div className="relative flex flex-col items-center">
            <span
              className={cn(
                "mb-6 rounded-full px-3 py-1 text-[12px] font-semibold",
                isIdle ? "bg-emerald-500/15 text-emerald-400" : meta.badge
              )}
            >
              {isIdle ? "待开始" : isRunning ? "进行中" : "已暂停"}
            </span>

            <div className="relative flex h-[260px] w-[260px] items-center justify-center">
              <svg className="absolute inset-0 -rotate-90" viewBox="0 0 240 240">
                <circle
                  cx="120"
                  cy="120"
                  r={radius}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="10"
                  className="text-foreground/8"
                />
                <circle
                  cx="120"
                  cy="120"
                  r={radius}
                  fill="none"
                  strokeWidth="10"
                  strokeLinecap="round"
                  strokeDasharray={circumference}
                  strokeDashoffset={dashOffset}
                  className={cn(
                    "transition-[stroke-dashoffset] duration-1000 ease-linear",
                    isIdle ? "stroke-emerald-400/40" : meta.ring
                  )}
                />
              </svg>

              <div className="text-center">
                <p className="font-mono text-6xl font-extrabold tabular-nums tracking-tight">
                  {formatClock(remainingSeconds)}
                </p>
                {!isIdle && (
                  <p className="mt-2 text-[12px] text-muted-foreground">
                    第 {Math.min(state.cycle_count + (state.phase === "work" ? 1 : 0), settings.pomodoro_sessions_per_cycle)} / {settings.pomodoro_sessions_per_cycle} 轮
                  </p>
                )}
              </div>
            </div>

            <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
              {isIdle || state.status === "paused" ? (
                <Button size="lg" className="min-w-32 gap-2" onClick={handleStart}>
                  <Play className="h-4 w-4" />
                  {isIdle ? "开始专注" : "继续"}
                </Button>
              ) : (
                <Button size="lg" variant="secondary" className="min-w-32 gap-2" onClick={handlePause}>
                  <Pause className="h-4 w-4" />
                  暂停
                </Button>
              )}

              {!isIdle && (
                <>
                  <Button size="lg" variant="outline" className="gap-2" onClick={handleSkip}>
                    <SkipForward className="h-4 w-4" />
                    跳过
                  </Button>
                  <Button size="lg" variant="ghost" className="gap-2" onClick={handleStop}>
                    <RotateCcw className="h-4 w-4" />
                    重置
                  </Button>
                </>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      <div className="grid grid-cols-3 gap-3">
        <PresetCard
          label="专注"
          minutes={settings.pomodoro_work_minutes}
          active={!isIdle && state.phase === "work"}
        />
        <PresetCard
          label="短休"
          minutes={settings.pomodoro_short_break_minutes}
          active={!isIdle && state.phase === "short_break"}
        />
        <PresetCard
          label="长休"
          minutes={settings.pomodoro_long_break_minutes}
          active={!isIdle && state.phase === "long_break"}
        />
      </div>

      <div className="glass-subtle flex items-center gap-3 rounded-lg px-4 py-3 text-[12px] text-muted-foreground">
        <Timer className="h-4 w-4 shrink-0 text-primary" />
        计时在后台持续运行，即使窗口最小化到托盘也不会中断。
      </div>
    </div>
  );
}

function PresetCard({
  label,
  minutes,
  active,
}: {
  label: string;
  minutes: number;
  active: boolean;
}) {
  return (
    <Card className={cn("overflow-hidden transition-colors", active && "ring-1 ring-primary/40")}>
      <CardContent className="p-4">
        <p className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {label}
        </p>
        <p className="mt-1 text-xl font-bold tabular-nums">{minutes} 分钟</p>
      </CardContent>
    </Card>
  );
}
