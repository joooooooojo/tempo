import { useEffect, useState } from "react";
import { zhCN } from "date-fns/locale";
import { CalendarClock, Repeat, SlidersHorizontal, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { recurrenceOptions } from "@/builtin-plugins/todo/lib/todoMeta";
import { cn } from "@/lib/utils";
import type { TodoRecurrence } from "@/types";
import { TagDraftList } from "@/components/ui/tag";

const DEFAULT_DUE_HOUR = "18";
const DEFAULT_DUE_MINUTE = "00";
const hourOptions = ["08", "09", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "22", "23"];
const minuteOptions = ["00", "15", "30", "45"];

function hasMoreSettings(tags: string[], recurrence: TodoRecurrence, dueAt: string) {
  return tags.length > 0 || recurrence !== "none" || Boolean(dueAt);
}

export function MoreSettingsDialog({
  tags,
  tagSuggestions = [],
  recurrence,
  dueAt,
  remind1d = false,
  remind1h = false,
  remindCustomHours = null,
  onTagsChange,
  onRecurrenceChange,
  onDueAtChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
}: {
  tags: string[];
  tagSuggestions?: string[];
  recurrence: TodoRecurrence;
  dueAt: string;
  remind1d?: boolean;
  remind1h?: boolean;
  remindCustomHours?: number | null;
  onTagsChange?: (value: string[]) => void;
  onRecurrenceChange?: (value: TodoRecurrence) => void;
  onDueAtChange: (value: string) => void;
  onRemind1dChange?: (value: boolean) => void;
  onRemind1hChange?: (value: boolean) => void;
  onRemindCustomHoursChange?: (value: number | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const showTags = Boolean(onTagsChange);
  const showRecurrence = Boolean(onRecurrenceChange);
  const showDue = showRecurrence ? recurrence === "none" : true;
  const hasActive = hasMoreSettings(tags, recurrence, dueAt);

  if (!showTags && !showRecurrence) {
    return null;
  }

  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className={cn(
          "relative h-10 gap-1.5 px-3 text-muted-foreground hover:text-foreground",
          hasActive && "text-foreground"
        )}
        onClick={() => setOpen(true)}
      >
        <SlidersHorizontal className="h-4 w-4 shrink-0" />
        <span>更多配置</span>
        {hasActive && <span className="todo-more-settings-dot" aria-hidden />}
      </Button>

      <Dialog open={open} onOpenChange={setOpen} modal="trap-focus">
        <DialogPanel
          showOverlay={false}
          className="todo-create-dialog max-h-[min(520px,85vh)] max-w-[440px]"
          onOpenAutoFocus={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>更多配置</DialogTitle>
          </DialogHeader>

          <DialogContent className="flex flex-col gap-5">
            {showTags && (
              <TagDraftList
                items={tags}
                suggestions={tagSuggestions}
                onChange={onTagsChange!}
              />
            )}

            {showRecurrence && (
              <div className="flex flex-col gap-2">
                <p className="text-[12px] font-semibold text-muted-foreground">重复</p>
                <div className="flex flex-wrap gap-1.5">
                  {recurrenceOptions.map((option) => (
                    <button
                      key={option.value}
                      type="button"
                      className={cn(
                        "inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-[13px] font-medium transition-colors",
                        option.value === recurrence
                          ? "border-primary/35 bg-primary/10 text-primary"
                          : "border-border/70 bg-[var(--todo-field-bg)] text-foreground hover:border-primary/20"
                      )}
                      onClick={() => onRecurrenceChange?.(option.value)}
                    >
                      {option.value !== "none" && <Repeat className="size-3.5" />}
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {showDue && (
              <div className="flex flex-col gap-2">
                <p className="text-[12px] font-semibold text-muted-foreground">截止时间</p>
                <DueDateField
                  value={dueAt}
                  className="w-full"
                  bordered
                  remind1d={remind1d}
                  remind1h={remind1h}
                  remindCustomHours={remindCustomHours}
                  popoverSide="top"
                  onChange={onDueAtChange}
                  onRemind1dChange={onRemind1dChange}
                  onRemind1hChange={onRemind1hChange}
                  onRemindCustomHoursChange={onRemindCustomHoursChange}
                />
              </div>
            )}
          </DialogContent>

          <DialogFooter>
            <Button type="button" className="h-9 min-w-20" onClick={() => setOpen(false)}>
              完成
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </>
  );
}

export function DueDateField({
  value,
  className,
  bordered = false,
  compact = false,
  remind1d = false,
  remind1h = false,
  remindCustomHours = null,
  popoverSide = "bottom",
  onChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
}: {
  value: string;
  className?: string;
  bordered?: boolean;
  compact?: boolean;
  remind1d?: boolean;
  remind1h?: boolean;
  remindCustomHours?: number | null;
  popoverSide?: "top" | "bottom";
  onChange: (value: string) => void;
  onRemind1dChange?: (value: boolean) => void;
  onRemind1hChange?: (value: boolean) => void;
  onRemindCustomHoursChange?: (value: number | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [focused, setFocused] = useState(false);
  const [customReminderEnabled, setCustomReminderEnabled] = useState(false);
  const [customReminderDraft, setCustomReminderDraft] = useState("");
  const selectedDate = parseDateTimeLocalValue(value);
  const [visibleMonth, setVisibleMonth] = useState(() => startOfMonth(selectedDate ?? new Date()));
  const fallbackDate = getDefaultDueDate(new Date());
  const hour = selectedDate ? String(selectedDate.getHours()).padStart(2, "0") : String(fallbackDate.getHours()).padStart(2, "0");
  const minute = selectedDate
    ? String(Math.min(45, Math.floor(selectedDate.getMinutes() / 15) * 15)).padStart(2, "0")
    : String(fallbackDate.getMinutes()).padStart(2, "0");

  useEffect(() => {
    if (open) setVisibleMonth(startOfMonth(selectedDate ?? new Date()));
  }, [open, value]);

  useEffect(() => {
    if (!open) return;
    const enabled = remindCustomHours != null;
    setCustomReminderEnabled(enabled);
    setCustomReminderDraft(enabled ? String(remindCustomHours) : "");
  }, [open, remindCustomHours]);

  const commitCustomReminderHours = (raw: string) => {
    const trimmed = raw.trim();
    if (!trimmed) {
      onRemindCustomHoursChange?.(null);
      return;
    }
    const hours = Number.parseInt(trimmed, 10);
    if (Number.isNaN(hours)) return;
    onRemindCustomHoursChange?.(Math.min(168, Math.max(1, hours)));
  };

  const clearOtherReminders = (keep: "1d" | "1h" | "custom") => {
    if (keep !== "1d") onRemind1dChange?.(false);
    if (keep !== "1h") onRemind1hChange?.(false);
    if (keep !== "custom") {
      onRemindCustomHoursChange?.(null);
      setCustomReminderEnabled(false);
      setCustomReminderDraft("");
    }
  };

  const commit = (date: Date, nextHour = hour, nextMinute = minute) => {
    const next = resolveDueDate(date, nextHour, nextMinute);
    if (!next) return;
    onChange(toDateTimeLocalValue(next));
  };

  const baseDate = selectedDate ?? fallbackDate;
  const commitAndClose = (date: Date, nextHour = hour, nextMinute = minute) => {
    const next = resolveDueDate(date, nextHour, nextMinute);
    if (!next) return;
    onChange(toDateTimeLocalValue(next));
    setOpen(false);
  };
  const placeholder = "截止时间";
  const active = open || focused || Boolean(value);
  const useBorderedStyle = bordered;
  const isBaseDateDisabled = isDueDateDisabled(baseDate);
  const todayDisabled = isDueDateDisabled(new Date());
  const hasReminderOptions = Boolean(
    value && (onRemind1dChange || onRemind1hChange || onRemindCustomHoursChange)
  );
  const reminderSummary = formatDueReminderSummary(remind1d, remind1h, remindCustomHours);
  const hasReminder = Boolean(remind1d || remind1h || remindCustomHours);
  const showReminderLine = Boolean(value && reminderSummary && !compact);

  return (
    <div
      className={cn(
        "relative shrink-0",
        showReminderLine ? "min-h-10" : "h-9",
        className
      )}
    >
      <Popover modal open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "relative flex w-full min-w-0 items-center gap-2 rounded-lg px-3 text-left text-[14px] font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
              showReminderLine ? "min-h-10 py-1.5" : "h-9",
              useBorderedStyle
                ? cn(
                    "border border-border/70 bg-[var(--todo-field-bg)] shadow-sm shadow-emerald-950/[0.03] hover:brightness-[1.02]",
                    active && "border-primary/45",
                    compact && hasReminder && value && "border-primary/30 bg-primary/[0.035]"
                  )
                : "glass-subtle hover:bg-white/45 dark:hover:bg-white/8",
              value ? "pr-9" : "pr-3"
            )}
            aria-label={placeholder}
            title={compact && hasReminder ? reminderSummary || undefined : undefined}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
          >
            <span className="relative inline-flex shrink-0 items-center justify-center self-center">
              <CalendarClock
                className={cn(
                  "block h-4 w-4 shrink-0 transition-colors",
                  hasReminder && value ? "text-primary" : "text-muted-foreground"
                )}
              />
              {hasReminder && value && (
                <span
                  className="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full border border-[var(--todo-field-bg)] bg-primary shadow-[0_0_0_1px_rgba(16,185,129,0.25)]"
                  aria-hidden
                />
              )}
            </span>
            {showReminderLine ? (
              <span className="flex min-w-0 flex-1 flex-col justify-center gap-0.5 self-stretch">
                <span className="truncate leading-5 text-foreground">{formatDueFieldValue(value)}</span>
                <span className="truncate text-[11px] font-normal leading-none text-muted-foreground">
                  {reminderSummary}
                </span>
              </span>
            ) : (
              <span
                className={cn(
                  "min-w-0 flex-1 truncate leading-5",
                  value ? "text-foreground" : "text-muted-foreground"
                )}
              >
                {value ? formatDueFieldValue(value) : placeholder}
              </span>
            )}
            {value && (
              <span
                role="button"
                tabIndex={0}
                className="absolute right-1.5 top-1/2 z-10 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-foreground"
                aria-label="清除截止时间"
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  onChange("");
                  setOpen(false);
                }}
                onKeyDown={(event) => {
                  if (event.key !== "Enter" && event.key !== " ") return;
                  event.preventDefault();
                  event.stopPropagation();
                  onChange("");
                  setOpen(false);
                }}
              >
                <X className="block h-3.5 w-3.5 shrink-0" />
              </span>
            )}
          </button>
        </PopoverTrigger>

        <PopoverContent
          side={popoverSide}
          align="start"
          collisionPadding={16}
          overlayLayer
          onOpenAutoFocus={(event) => event.preventDefault()}
          className="flex w-fit max-h-[min(var(--available-height),calc(100dvh-2rem))] max-w-[calc(100vw-2.5rem)] flex-col gap-0 overflow-hidden p-0"
        >
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
            <div className="grid grid-cols-[max-content_132px] gap-3 p-3">
              <Calendar
                className="p-0"
                mode="single"
                locale={zhCN}
                formatters={{
                  formatCaption: (month) =>
                    `${month.getFullYear()}年${month.getMonth() + 1}月`,
                  formatWeekdayName: (weekday) =>
                    ["日", "一", "二", "三", "四", "五", "六"][weekday.getDay()],
                }}
                month={visibleMonth}
                selected={selectedDate}
                disabled={isDueDateDisabled}
                onMonthChange={setVisibleMonth}
                onSelect={(date) => {
                  if (date) commit(date, DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE);
                }}
              />
              <div className="rounded-lg border border-border/60 bg-foreground/[0.025] p-2.5">
                <div>
                  <p className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                    小时
                  </p>
                  <div className="grid grid-cols-4 gap-1">
                    {hourOptions.map((option) => {
                      const disabled = isBaseDateDisabled || isDueHourDisabled(baseDate, option);
                      return (
                        <button
                          key={option}
                          type="button"
                          disabled={disabled}
                          className={cn(
                            "h-7 rounded-md text-[12px] font-semibold transition-colors",
                            option === hour
                              ? "bg-primary text-primary-foreground shadow-sm shadow-primary/20"
                              : "bg-foreground/5 text-muted-foreground hover:bg-foreground/8 hover:text-foreground",
                            disabled && "cursor-default bg-foreground/5 text-muted-foreground/35 shadow-none hover:bg-foreground/5 hover:text-muted-foreground/35"
                          )}
                          onClick={() => commit(baseDate, option, firstSelectableMinute(baseDate, option, minute) ?? minute)}
                        >
                          {option}
                        </button>
                      );
                    })}
                  </div>
                </div>
                <div className="mt-3">
                  <p className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                    分钟
                  </p>
                  <div className="grid grid-cols-2 gap-1">
                    {minuteOptions.map((option) => {
                      const disabled = isBaseDateDisabled || isDueTimeDisabled(baseDate, hour, option);
                      return (
                        <button
                          key={option}
                          type="button"
                          disabled={disabled}
                          className={cn(
                            "h-7 rounded-md text-[12px] font-semibold transition-colors",
                            option === minute
                              ? "bg-primary text-primary-foreground shadow-sm shadow-primary/20"
                              : "bg-foreground/5 text-muted-foreground hover:bg-foreground/8 hover:text-foreground",
                            disabled && "cursor-default bg-foreground/5 text-muted-foreground/35 shadow-none hover:bg-foreground/5 hover:text-muted-foreground/35"
                          )}
                          onClick={() =>
                            hasReminderOptions
                              ? commit(baseDate, hour, option)
                              : commitAndClose(baseDate, hour, option)
                          }
                        >
                          {option}
                        </button>
                      );
                    })}
                  </div>
                </div>
              </div>
            </div>
            {hasReminderOptions && (
              <div className="border-t border-border/60 px-3 py-2">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                  <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                    提醒
                  </span>
                  <label className="flex items-center gap-1.5 text-[12px] text-foreground">
                    <input
                      type="checkbox"
                      checked={remind1d}
                      className="accent-primary"
                      onChange={(event) => {
                        if (event.target.checked) {
                          clearOtherReminders("1d");
                          onRemind1dChange?.(true);
                          return;
                        }
                        onRemind1dChange?.(false);
                      }}
                    />
                    提前 1 天
                  </label>
                  <label className="flex items-center gap-1.5 text-[12px] text-foreground">
                    <input
                      type="checkbox"
                      checked={remind1h}
                      className="accent-primary"
                      onChange={(event) => {
                        if (event.target.checked) {
                          clearOtherReminders("1h");
                          onRemind1hChange?.(true);
                          return;
                        }
                        onRemind1hChange?.(false);
                      }}
                    />
                    提前 1 小时
                  </label>
                  <label className="flex items-center gap-1.5 text-[12px] text-foreground">
                    <input
                      type="checkbox"
                      checked={customReminderEnabled}
                      className="accent-primary"
                      onChange={(event) => {
                        const enabled = event.target.checked;
                        if (enabled) {
                          clearOtherReminders("custom");
                          setCustomReminderEnabled(true);
                          return;
                        }
                        setCustomReminderEnabled(false);
                        setCustomReminderDraft("");
                        onRemindCustomHoursChange?.(null);
                      }}
                    />
                    <span className="flex items-center gap-1">
                      <input
                        type="text"
                        inputMode="numeric"
                        disabled={!customReminderEnabled}
                        placeholder="自定义"
                        value={customReminderDraft}
                        className="h-6 w-14 border-0 border-b border-border/80 bg-transparent px-0 text-center text-[12px] text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-40"
                        onFocus={() => {
                          if (!customReminderEnabled) {
                            clearOtherReminders("custom");
                            setCustomReminderEnabled(true);
                          }
                        }}
                        onChange={(event) => {
                          const next = event.target.value.replace(/[^\d]/g, "");
                          setCustomReminderDraft(next);
                          clearOtherReminders("custom");
                          setCustomReminderEnabled(true);
                          if (!next) {
                            onRemindCustomHoursChange?.(null);
                            return;
                          }
                          commitCustomReminderHours(next);
                        }}
                        onBlur={() => commitCustomReminderHours(customReminderDraft)}
                      />
                      <span className="text-muted-foreground">小时</span>
                    </span>
                  </label>
                </div>
              </div>
            )}
          </div>
          <div className="shrink-0 border-t border-border/60 bg-popover p-2.5">
            <div className="flex items-center justify-between gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-8 text-muted-foreground"
                onClick={() => {
                  onChange("");
                  setOpen(false);
                }}
              >
                清除
              </Button>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-8"
                  disabled={todayDisabled}
                  onClick={() => commit(new Date(), DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE)}
                >
                  今天
                </Button>
                <Button
                  type="button"
                  size="sm"
                  className="h-8"
                  onClick={() => {
                    if (!selectedDate || isDueTimeDisabled(selectedDate, hour, minute)) {
                      commit(
                        fallbackDate,
                        String(fallbackDate.getHours()).padStart(2, "0"),
                        String(fallbackDate.getMinutes()).padStart(2, "0")
                      );
                    }
                    setOpen(false);
                  }}
                >
                  完成
                </Button>
              </div>
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}

function formatDueFieldValue(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "截止时间";

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatDueReminderSummary(remind1d: boolean, remind1h: boolean, remindCustomHours?: number | null) {
  const parts: string[] = [];
  if (remind1d) parts.push("提前 1 天");
  if (remind1h) parts.push("提前 1 小时");
  if (remindCustomHours) parts.push(`提前 ${remindCustomHours} 小时`);
  return parts.join(" · ");
}

function parseDateTimeLocalValue(value?: string | null) {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function isSameLocalDay(a: Date, b: Date) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function dateWithTime(date: Date, hour: string, minute: string) {
  const next = new Date(date);
  next.setHours(Number(hour), Number(minute), 0, 0);
  return next;
}

function isDueDateDisabled(date: Date) {
  const now = new Date();
  const day = startOfDay(date);
  const today = startOfDay(now);

  if (day.getTime() < today.getTime()) return true;
  if (day.getTime() > today.getTime()) return false;

  return !minuteOptions.some((minute) =>
    hourOptions.some((hour) => dateWithTime(date, hour, minute).getTime() >= now.getTime())
  );
}

function isDueTimeDisabled(date: Date, hour: string, minute: string) {
  const now = new Date();
  if (!isSameLocalDay(date, now)) return isDueDateDisabled(date);
  return dateWithTime(date, hour, minute).getTime() < now.getTime();
}

function isDueHourDisabled(date: Date, hour: string) {
  return minuteOptions.every((minute) => isDueTimeDisabled(date, hour, minute));
}

function firstSelectableMinute(date: Date, hour: string, preferredMinute: string) {
  if (!isDueTimeDisabled(date, hour, preferredMinute)) return preferredMinute;
  return minuteOptions.find((minute) => !isDueTimeDisabled(date, hour, minute)) ?? null;
}

function firstSelectableDateTime(date: Date) {
  for (const hour of hourOptions) {
    for (const minute of minuteOptions) {
      if (!isDueTimeDisabled(date, hour, minute)) {
        return dateWithTime(date, hour, minute);
      }
    }
  }

  const tomorrow = new Date(date);
  tomorrow.setDate(date.getDate() + 1);
  return dateWithTime(tomorrow, hourOptions[0], minuteOptions[0]);
}

function getDefaultDueDate(now: Date) {
  const defaultToday = dateWithTime(now, DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE);
  if (defaultToday.getTime() >= now.getTime()) return defaultToday;
  return firstSelectableDateTime(now);
}

function resolveDueDate(date: Date, hour: string, minute: string) {
  if (isDueDateDisabled(date)) return null;

  const preferred = dateWithTime(date, hour, minute);
  if (!isDueTimeDisabled(date, hour, minute)) return preferred;

  return firstSelectableDateTime(date);
}

function toDateTimeLocalValue(value?: string | Date | null) {
  if (!value) return "";
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";

  const offset = date.getTimezoneOffset();
  const localDate = new Date(date.getTime() - offset * 60_000);
  return localDate.toISOString().slice(0, 16);
}
