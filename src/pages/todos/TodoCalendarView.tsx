import { useCallback, useEffect, useMemo, useState, type AnimationEvent } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { cn } from "@/lib/utils";
import type { TodoItem } from "@/types";
import { dueBadgeClass, formatTodoDate } from "./todoPageUtils";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"] as const;

function startOfLocalDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function localDayKey(date: Date) {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

function isoDayKey(value?: string | null) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return localDayKey(date);
}

function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function addMonths(date: Date, delta: number) {
  return new Date(date.getFullYear(), date.getMonth() + delta, 1);
}

function groupTodosByDay(
  todos: TodoItem[],
  keyOf: (todo: TodoItem) => string | null
) {
  const map = new Map<string, TodoItem[]>();
  for (const todo of todos) {
    const key = keyOf(todo);
    if (!key) continue;
    const list = map.get(key) ?? [];
    list.push(todo);
    map.set(key, list);
  }
  return map;
}

/** Monday-first month grid (always 6 weeks for stable layout). */
function buildMonthCells(month: Date) {
  const first = startOfMonth(month);
  const mondayOffset = (first.getDay() + 6) % 7;
  const gridStart = new Date(first);
  gridStart.setDate(first.getDate() - mondayOffset);

  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(gridStart);
    date.setDate(gridStart.getDate() + index);
    return startOfLocalDay(date);
  });
}

function hashUnit(seed: number) {
  const x = Math.sin(seed * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

type OrbitLayout = {
  x: number;
  y: number;
  rot: number;
  delay: number;
  duration: number;
};

function orbitLayouts(count: number, seedBase: number): OrbitLayout[] {
  if (count <= 0) return [];

  const needsCenter = count === 1 || count > 3;
  const ringCount = needsCenter ? count - 1 : count;
  const radiusBase = ringCount <= 0 ? 0 : Math.min(36, 18 + ringCount * 2.8);

  const makeMeta = (index: number) => ({
    rot: (hashUnit(seedBase + index * 41) - 0.5) * 14,
    delay: 0.04 + hashUnit(seedBase + index * 7) * 0.28,
    duration: 3.2 + hashUnit(seedBase + index * 11) * 2.4,
  });

  return Array.from({ length: count }, (_, index) => {
    const meta = makeMeta(index);

    if (needsCenter && index === 0) {
      return {
        x: 50,
        y: 50,
        rot: (hashUnit(seedBase + 3) - 0.5) * 6,
        delay: 0.02,
        duration: 4.2,
      };
    }

    const ringIndex = needsCenter ? index - 1 : index;
    const angle =
      (ringIndex / ringCount) * Math.PI * 2 -
      Math.PI / 2 +
      (hashUnit(seedBase + ringIndex * 17) - 0.5) * 0.45;
    const radius = radiusBase + (hashUnit(seedBase + ringIndex * 29) - 0.5) * 12;

    return {
      x: 50 + Math.cos(angle) * radius,
      y: 50 + Math.sin(angle) * radius * 0.88,
      ...meta,
    };
  });
}

type DayRelation = {
  todo: TodoItem;
  isDue: boolean;
  isCreated: boolean;
};

export function TodoCalendarView({
  todos,
  onOpenDetail,
}: {
  todos: TodoItem[];
  onOpenDetail: (todo: TodoItem) => void;
}) {
  const today = useMemo(() => startOfLocalDay(new Date()), []);
  const [month, setMonth] = useState(() => startOfMonth(today));
  const [activeDay, setActiveDay] = useState<Date | null>(null);
  const [orbitState, setOrbitState] = useState<"open" | "closed">("closed");

  const todosByDueDay = useMemo(
    () => groupTodosByDay(todos, (todo) => isoDayKey(todo.due_at)),
    [todos]
  );

  const todosByCreatedDay = useMemo(
    () => groupTodosByDay(todos, (todo) => isoDayKey(todo.created_at)),
    [todos]
  );

  const cells = useMemo(() => buildMonthCells(month), [month]);
  const activeKey = activeDay ? localDayKey(activeDay) : null;
  const orbitOpen = Boolean(activeDay);

  const activeRelations = useMemo(() => {
    if (!activeKey) return [] as DayRelation[];
    const dueList = todosByDueDay.get(activeKey) ?? [];
    const createdList = todosByCreatedDay.get(activeKey) ?? [];
    const byId = new Map<number, DayRelation>();

    for (const todo of dueList) {
      byId.set(todo.id, { todo, isDue: true, isCreated: false });
    }
    for (const todo of createdList) {
      const existing = byId.get(todo.id);
      if (existing) {
        existing.isCreated = true;
      } else {
        byId.set(todo.id, { todo, isDue: false, isCreated: true });
      }
    }

    return Array.from(byId.values());
  }, [activeKey, todosByCreatedDay, todosByDueDay]);

  const layouts = useMemo(() => {
    if (!activeDay) return [];
    const seed =
      activeDay.getFullYear() * 10000 +
      (activeDay.getMonth() + 1) * 100 +
      activeDay.getDate();
    return orbitLayouts(activeRelations.length, seed);
  }, [activeDay, activeRelations.length]);

  const openOrbit = (date: Date) => {
    setActiveDay(date);
    setOrbitState("open");
  };

  const closeOrbit = useCallback(() => {
    setOrbitState((state) => {
      if (state === "closed") return state;
      const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (reduceMotion) {
        setActiveDay(null);
      }
      return "closed";
    });
  }, []);

  const handleOrbitAnimationEnd = (event: AnimationEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (orbitState !== "closed") return;
    if (event.animationName !== "todo-orbit-overlay-out") return;
    setActiveDay(null);
  };

  useEffect(() => {
    if (!orbitOpen || orbitState !== "open") return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeOrbit();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeOrbit, orbitOpen, orbitState]);

  const monthLabel = month.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "long",
  });

  return (
    <div className="todo-month-calendar relative flex h-full min-h-0 w-full flex-col">
      <header className="mb-3 flex shrink-0 items-center justify-between gap-3">
        <button
          type="button"
          className="inline-flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          aria-label="上个月"
          onClick={() => setMonth((current) => addMonths(current, -1))}
        >
          <ChevronLeft className="size-4" />
        </button>
        <h2 className="text-[15px] font-semibold tracking-wide text-foreground">
          {monthLabel}
        </h2>
        <button
          type="button"
          className="inline-flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          aria-label="下个月"
          onClick={() => setMonth((current) => addMonths(current, 1))}
        >
          <ChevronRight className="size-4" />
        </button>
      </header>

      <div className="mb-1.5 flex shrink-0 items-center justify-end gap-3 px-0.5 text-[10px] text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <span className="size-1.5 rounded-full bg-primary" />
          截止
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="size-1.5 rounded-full bg-sky-500" />
          创建
        </span>
      </div>

      <div className="mb-2 grid shrink-0 grid-cols-7 gap-1.5">
        {WEEKDAYS.map((label) => (
          <div
            key={label}
            className="py-1 text-center text-[11px] font-medium text-muted-foreground"
          >
            {label}
          </div>
        ))}
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6 gap-1.5">
        {cells.map((date) => {
          const key = localDayKey(date);
          const dueCount = todosByDueDay.get(key)?.length ?? 0;
          const createdCount = todosByCreatedDay.get(key)?.length ?? 0;
          const inMonth = date.getMonth() === month.getMonth();
          const isToday = key === localDayKey(today);
          const isActive = activeKey === key;
          const hasMarkers = dueCount > 0 || createdCount > 0;

          return (
            <button
              key={key}
              type="button"
              onClick={() => openOrbit(date)}
              className={cn(
                "todo-month-calendar__cell group relative flex min-h-0 flex-col items-start justify-between rounded-2xl border px-2.5 py-2 text-left transition-colors",
                inMonth
                  ? "border-border/50 bg-background/70 hover:border-primary/35 hover:bg-primary/[0.04]"
                  : "border-transparent bg-foreground/[0.02] text-muted-foreground/55 hover:bg-foreground/[0.04]",
                isToday && inMonth && "border-primary/40 bg-primary/[0.06]",
                isActive && "border-primary/50 bg-primary/[0.09] ring-2 ring-primary/20"
              )}
            >
              <span
                className={cn(
                  "inline-flex size-7 items-center justify-center rounded-full text-[13px] font-medium tabular-nums",
                  isToday && "bg-primary text-primary-foreground",
                  !isToday && inMonth && "text-foreground",
                  !inMonth && "text-muted-foreground/50"
                )}
              >
                {date.getDate()}
              </span>
              {hasMarkers ? (
                <div className="flex flex-wrap items-center gap-1">
                  {dueCount > 0 && (
                    <span
                      className={cn(
                        "inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-semibold tabular-nums",
                        isToday
                          ? "bg-primary/15 text-primary"
                          : "bg-primary/10 text-primary"
                      )}
                      title={`${dueCount} 项截止`}
                    >
                      <span className="size-1 rounded-full bg-primary opacity-80" />
                      {dueCount}
                    </span>
                  )}
                  {createdCount > 0 && (
                    <span
                      className="inline-flex items-center gap-1 rounded-full bg-sky-500/12 px-1.5 py-0.5 text-[10px] font-semibold tabular-nums text-sky-700 dark:text-sky-300"
                      title={`${createdCount} 项创建`}
                    >
                      <span className="size-1 rounded-full bg-sky-500 opacity-80" />
                      {createdCount}
                    </span>
                  )}
                </div>
              ) : (
                <span className="h-[18px]" />
              )}
            </button>
          );
        })}
      </div>

      {orbitOpen && activeDay && (
        <div
          className="todo-month-calendar__orbit absolute inset-0 z-20 overflow-hidden rounded-2xl"
          data-state={orbitState}
          role="dialog"
          aria-modal="true"
          aria-label={`${activeDay.toLocaleDateString("zh-CN", {
            month: "long",
            day: "numeric",
          })} 的待办`}
          onAnimationEnd={handleOrbitAnimationEnd}
        >
          <button
            type="button"
            className="todo-month-calendar__orbit-backdrop absolute inset-0 z-0 cursor-default bg-background/72 backdrop-blur-[2px]"
            aria-label="关闭"
            onClick={closeOrbit}
          />

          <div className="todo-month-calendar__orbit-header pointer-events-none absolute inset-x-0 top-0 z-10 flex items-start justify-between gap-3 p-4">
            <div>
              <p className="text-[13px] font-semibold text-foreground">
                {activeDay.toLocaleDateString("zh-CN", {
                  month: "long",
                  day: "numeric",
                  weekday: "short",
                })}
              </p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                {activeRelations.length > 0
                  ? `${activeRelations.length} 项相关待办 · 点击卡片查看详情`
                  : "这一天没有相关待办 · 点击空白处返回"}
              </p>
            </div>
            <button
              type="button"
              className="pointer-events-auto inline-flex size-8 items-center justify-center rounded-full border border-border/60 bg-background/90 text-muted-foreground shadow-sm transition-colors hover:text-foreground"
              aria-label="关闭日视图"
              onClick={closeOrbit}
            >
              <X className="size-4" />
            </button>
          </div>

          <div className="pointer-events-none absolute inset-0 z-[1]">
            {activeRelations.length === 0 ? (
              <div className="todo-month-calendar__orbit-empty absolute top-1/2 left-1/2 w-[min(240px,70%)] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-dashed border-border/70 bg-background/80 px-5 py-8 text-center text-[12px] text-muted-foreground shadow-sm">
                这一天没有相关待办
                <span className="mt-1.5 block text-[11px] text-muted-foreground/80">
                  点击空白处返回日历
                </span>
              </div>
            ) : (
              activeRelations.map((relation, index) => {
                const layout = layouts[index] ?? layouts[0];
                const { todo, isDue, isCreated } = relation;
                const exitDelayMs = Math.max(0, (activeRelations.length - 1 - index) * 35);
                return (
                  <button
                    key={todo.id}
                    type="button"
                    className={cn(
                      "todo-month-calendar__orbit-card pointer-events-auto absolute max-w-[220px] min-w-[148px] rounded-3xl border bg-background/95 px-3.5 py-3 text-left shadow-[0_10px_30px_-12px_rgba(15,40,30,0.35)] backdrop-blur-sm transition-[box-shadow,transform] hover:z-10 hover:shadow-[0_16px_36px_-12px_rgba(15,40,30,0.42)] focus-visible:outline-none focus-visible:ring-2",
                      isDue && isCreated && "border-primary/45 ring-1 ring-sky-400/35 focus-visible:ring-primary/40",
                      isDue && !isCreated && "border-primary/50 focus-visible:ring-primary/40",
                      !isDue && isCreated && "border-sky-400/55 focus-visible:ring-sky-400/40"
                    )}
                    style={{
                      left: `${layout.x}%`,
                      top: `${layout.y}%`,
                      ["--orbit-rot" as string]: `${layout.rot}deg`,
                      ["--orbit-enter-delay" as string]: `${40 + index * 55}ms`,
                      ["--orbit-exit-delay" as string]: `${exitDelayMs}ms`,
                      ["--orbit-wobble-delay" as string]: `${layout.delay}s`,
                      ["--orbit-wobble-duration" as string]: `${layout.duration}s`,
                    }}
                    onClick={() => onOpenDetail(todo)}
                  >
                    <p
                      className={cn(
                        "line-clamp-2 text-[13px] font-semibold leading-snug",
                        todo.completed && "text-muted-foreground line-through"
                      )}
                    >
                      {todo.title}
                    </p>
                    <div className="mt-2 flex flex-wrap items-center gap-1">
                      {isDue && (
                        <span
                          className={cn(
                            "inline-flex rounded-full px-1.5 py-0.5 text-[10px] font-medium",
                            dueBadgeClass(todo)
                          )}
                        >
                          截止{todo.due_at ? ` ${formatTodoDate(todo.due_at)}` : ""}
                        </span>
                      )}
                      {isCreated && (
                        <span className="inline-flex rounded-full bg-sky-500/12 px-1.5 py-0.5 text-[10px] font-medium text-sky-700 dark:text-sky-300">
                          创建
                        </span>
                      )}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
