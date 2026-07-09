import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { Pause, Play, RotateCcw, Settings2, SkipForward, SlidersHorizontal } from "lucide-react";
import { toast } from "sonner";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { api } from "@/lib/api";
import { cn, formatClock } from "@/lib/utils";
import type { PomodoroState, Settings, TodoItem } from "@/types";

const phaseMeta = {
  work: {
    label: "专注",
    ring: "stroke-emerald-400",
    glow: "from-emerald-400/20 to-teal-500/10",
    badge: "bg-emerald-500/15 text-emerald-500 dark:text-emerald-300",
  },
  short_break: {
    label: "短休",
    ring: "stroke-sky-400",
    glow: "from-sky-400/20 to-blue-500/10",
    badge: "bg-sky-500/15 text-sky-500 dark:text-sky-300",
  },
  long_break: {
    label: "长休",
    ring: "stroke-violet-400",
    glow: "from-violet-400/20 to-purple-500/10",
    badge: "bg-violet-500/15 text-violet-500 dark:text-violet-300",
  },
} as const;

export function PomodoroPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [state, setState] = useState<PomodoroState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [todos, setTodos] = useState<TodoItem[]>([]);

  const activeTodos = useMemo(
    () => todos.filter((todo) => !todo.completed),
    [todos]
  );

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [nextState, nextSettings, nextTodos] = await Promise.all([
        api.getPomodoroState(),
        api.getSettings(),
        api.getTodos(),
      ]);
      if (!cancelled) {
        setState(nextState);
        setSettings(nextSettings);
        setTodos(nextTodos);
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
    const todoParam = searchParams.get("todo");
    if (!todoParam || !state || state.status !== "idle") return;

    const todoId = Number(todoParam);
    if (!Number.isFinite(todoId)) return;

    void (async () => {
      try {
        const nextState = await api.setPomodoroTodo(todoId);
        setState(nextState);
      } catch (error) {
        console.error(error);
        toast.error(error instanceof Error ? error.message : "绑定待办失败");
      } finally {
        setSearchParams({}, { replace: true });
      }
    })();
  }, [searchParams, setSearchParams, state?.status]);

  if (!state || !settings) {
    return (
      <div className="flex h-[calc(100vh-5rem)] items-center justify-center overflow-hidden">
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
  const selectedTodoValue =
    state.active_todo_id === null ? "none" : String(state.active_todo_id);
  const phaseLabel = isIdle ? "待开始" : isRunning ? meta.label : "已暂停";
  const cycleLabel = `${Math.min(
    state.cycle_count + (state.phase === "work" ? 1 : 0),
    settings.pomodoro_sessions_per_cycle
  )} / ${settings.pomodoro_sessions_per_cycle}`;

  const handleSelectTodo = async (value: string) => {
    try {
      const todoId = value === "none" ? null : Number(value);
      setState(await api.setPomodoroTodo(todoId));
    } catch (error) {
      console.error(error);
      toast.error(error instanceof Error ? error.message : "更换待办失败");
    }
  };

  const handleUpdateSettings = async (patch: Partial<Settings>) => {
    const previous = settings;
    setSettings({ ...settings, ...patch });
    try {
      await api.updateSettings(patch);
    } catch (error) {
      console.error(error);
      setSettings(previous);
      toast.error(error instanceof Error ? error.message : "保存配置失败");
    }
  };

  const handleStart = async () => {
    try {
      setState(await api.startPomodoro(state.active_todo_id));
    } catch (error) {
      console.error(error);
      toast.error(error instanceof Error ? error.message : "无法开始专注");
    }
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
    <div className="mx-auto flex h-[calc(100vh-5rem)] max-h-[calc(100vh-5rem)] max-w-3xl flex-col gap-3 overflow-hidden">
      <div className="flex shrink-0 items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              "inline-flex h-8 shrink-0 items-center rounded-lg px-3 text-[12px] font-semibold",
              isIdle ? "bg-emerald-500/15 text-emerald-500 dark:text-emerald-300" : meta.badge
            )}
          >
            {phaseLabel}
          </span>
          <span className="hidden truncate text-[12px] font-medium text-muted-foreground sm:block">
            {settings.pomodoro_work_minutes} 分钟专注 · {settings.pomodoro_short_break_minutes} 分钟短休 · 每 {settings.pomodoro_sessions_per_cycle} 轮长休
          </span>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <div className="glass-subtle flex h-8 items-center gap-2 rounded-lg px-3">
            <span className="text-[11px] text-muted-foreground">今日完成</span>
            <span className="text-[18px] font-bold leading-none tabular-nums text-primary">
              {state.sessions_today}
            </span>
          </div>
          <PomodoroSettingsDialog
            settings={settings}
            onChange={(patch) => void handleUpdateSettings(patch)}
          />
        </div>
      </div>

      <Card className="glass-glow min-h-0 flex-1 overflow-hidden">
        <CardContent className="relative flex h-full min-h-0 flex-col p-4 sm:p-5">
          <div
            className={cn(
              "pointer-events-none absolute inset-0 bg-gradient-to-br opacity-70",
              isIdle ? "from-emerald-400/10 to-teal-500/5" : meta.glow
            )}
          />

          <div className="relative grid min-h-0 flex-1 grid-rows-[auto_1fr_auto] gap-3">
            <div className="grid shrink-0 gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
              <div className="glass-subtle min-w-0 rounded-lg p-3">
                {isIdle ? (
                  <div className="grid gap-2 sm:grid-cols-[auto_minmax(0,1fr)] sm:items-center">
                    <Label className="text-[12px] font-semibold text-muted-foreground">
                      绑定待办
                    </Label>
                    <Select value={selectedTodoValue} onValueChange={handleSelectTodo}>
                      <SelectTrigger className="h-9 w-full border-0 bg-foreground/[0.04] text-[13px] shadow-none">
                        <SelectValue placeholder="选择要专注的待办" />
                      </SelectTrigger>
                      <SelectContent searchable searchPlaceholder="搜索待办">
                        <SelectItem value="none">不绑定待办</SelectItem>
                        {activeTodos.map((todo) => (
                          <SelectItem key={todo.id} value={String(todo.id)}>
                            {todo.title}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                ) : (
                  <div className="flex min-h-9 items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="text-[11px] font-semibold text-muted-foreground">当前待办</p>
                      <p className="truncate text-[14px] font-semibold">
                        {state.active_todo_title ?? "未绑定待办"}
                      </p>
                    </div>
                    <span className="shrink-0 rounded-md bg-foreground/[0.04] px-2 py-1 text-[12px] text-muted-foreground">
                      第 {cycleLabel} 轮
                    </span>
                  </div>
                )}
              </div>

              <div className="hidden grid-cols-4 gap-2 sm:grid">
                <CompactMetric label="专注" value={`${settings.pomodoro_work_minutes}m`} active={!isIdle && state.phase === "work"} />
                <CompactMetric label="短休" value={`${settings.pomodoro_short_break_minutes}m`} active={!isIdle && state.phase === "short_break"} />
                <CompactMetric label="长休" value={`${settings.pomodoro_long_break_minutes}m`} active={!isIdle && state.phase === "long_break"} />
                <CompactMetric label="长休间隔" value={`${settings.pomodoro_sessions_per_cycle}轮`} active={false} />
              </div>
            </div>

            <div className="flex min-h-0 items-center justify-center py-1">
              <div className="relative flex h-[clamp(190px,38vh,252px)] w-[clamp(190px,38vh,252px)] items-center justify-center">
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
                  <p className="font-mono text-5xl font-extrabold leading-none tabular-nums tracking-normal sm:text-6xl">
                    {formatClock(remainingSeconds)}
                  </p>
                  <p className="mt-2 text-[12px] font-medium text-muted-foreground">
                    {isIdle ? `${settings.pomodoro_work_minutes} 分钟` : `第 ${cycleLabel} 轮`}
                  </p>
                </div>
              </div>
            </div>

            <div className="flex shrink-0 flex-wrap items-center justify-center gap-2">
              {isIdle || state.status === "paused" ? (
                <Button size="lg" className="min-w-32 gap-2" onClick={() => void handleStart()}>
                  <Play className="h-4 w-4" />
                  {isIdle ? "开始专注" : "继续"}
                </Button>
              ) : (
                <Button size="lg" variant="secondary" className="min-w-32 gap-2" onClick={() => void handlePause()}>
                  <Pause className="h-4 w-4" />
                  暂停
                </Button>
              )}

              {!isIdle && (
                <>
                  <Button size="lg" variant="outline" className="gap-2" onClick={() => void handleSkip()}>
                    <SkipForward className="h-4 w-4" />
                    跳过
                  </Button>
                  <Button size="lg" variant="ghost" className="gap-2" onClick={() => void handleStop()}>
                    <RotateCcw className="h-4 w-4" />
                    重置
                  </Button>
                </>
              )}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function PomodoroSettingsDialog({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
}) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="h-8 gap-1.5 px-3">
          <SlidersHorizontal className="h-3.5 w-3.5" />
          配置
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-[420px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings2 className="h-4 w-4 text-primary" />
            番茄配置
          </DialogTitle>
          <DialogDescription className="sr-only">
            调整番茄时钟的专注、休息和长休间隔。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5">
          <PomodoroSlider
            label="专注时长"
            value={settings.pomodoro_work_minutes}
            min={5}
            max={60}
            step={5}
            suffix="分钟"
            onChange={(value) => onChange({ pomodoro_work_minutes: value })}
          />
          <PomodoroSlider
            label="短休时长"
            value={settings.pomodoro_short_break_minutes}
            min={1}
            max={15}
            step={1}
            suffix="分钟"
            onChange={(value) => onChange({ pomodoro_short_break_minutes: value })}
          />
          <PomodoroSlider
            label="长休时长"
            value={settings.pomodoro_long_break_minutes}
            min={5}
            max={30}
            step={5}
            suffix="分钟"
            onChange={(value) => onChange({ pomodoro_long_break_minutes: value })}
          />
          <PomodoroSlider
            label="长休间隔"
            value={settings.pomodoro_sessions_per_cycle}
            min={2}
            max={8}
            step={1}
            suffix="轮"
            onChange={(value) => onChange({ pomodoro_sessions_per_cycle: value })}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PomodoroSlider({
  label,
  value,
  min,
  max,
  step,
  suffix,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix: string;
  onChange: (value: number) => void;
}) {
  return (
    <div>
      <div className="mb-3 flex items-center justify-between gap-4">
        <Label className="text-[13px] font-medium">{label}</Label>
        <span className="rounded-md bg-foreground/[0.04] px-2 py-1 text-[12px] font-semibold tabular-nums text-muted-foreground">
          {value} {suffix}
        </span>
      </div>
      <Slider
        min={min}
        max={max}
        step={step}
        value={[value]}
        onValueChange={([nextValue]) => onChange(nextValue)}
      />
    </div>
  );
}

function CompactMetric({
  label,
  value,
  active,
}: {
  label: string;
  value: string;
  active: boolean;
}) {
  return (
    <div
      className={cn(
        "glass-subtle rounded-lg px-3 py-2 text-center transition-colors",
        active && "bg-primary/12 text-primary ring-1 ring-primary/25"
      )}
    >
      <p className="text-[10px] font-medium text-muted-foreground">{label}</p>
      <p className="mt-0.5 text-[13px] font-bold tabular-nums">{value}</p>
    </div>
  );
}
