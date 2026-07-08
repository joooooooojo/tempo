import {
  useEffect,
  useRef,
  useState,
  type ChangeEventHandler,
  type ClipboardEvent,
  type FormEvent,
  type ReactNode,
} from "react";
import { CalendarClock, ImagePlus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { TodoImageInput } from "@/lib/api";
import { cn } from "@/lib/utils";

export interface DraftTodoImage extends TodoImageInput {
  local_id: string;
}

type TodoCreateDialogProps = {
  open: boolean;
  heading?: string;
  todoTitle: string;
  dueAt: string;
  images: DraftTodoImage[];
  saving?: boolean;
  titlePlaceholder?: string;
  submitLabel?: string;
  onOpenChange: (open: boolean) => void;
  onTitleChange: (value: string) => void;
  onDueAtChange: (value: string) => void;
  onDeleteImage: (image: DraftTodoImage) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

type TodoCreateFormPanelProps = Omit<TodoCreateDialogProps, "open" | "onOpenChange"> & {
  titleElement?: ReactNode;
  cancelElement?: ReactNode;
  onCancel?: () => void;
};

const DEFAULT_DUE_HOUR = "18";
const DEFAULT_DUE_MINUTE = "00";
const hourOptions = ["08", "09", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "22", "23"];
const minuteOptions = ["00", "15", "30", "45"];

export function TodoCreateDialog({
  open,
  heading = "新建待办",
  todoTitle,
  dueAt,
  images,
  saving = false,
  titlePlaceholder = "待办内容",
  submitLabel = "创建",
  onOpenChange,
  onTitleChange,
  onDueAtChange,
  onDeleteImage,
  onSubmit,
}: TodoCreateDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="todo-create-dialog max-w-[520px] gap-0 overflow-visible rounded-xl border-border/80 p-0"
      >
        <TodoCreateFormPanel
          heading={heading}
          todoTitle={todoTitle}
          dueAt={dueAt}
          images={images}
          saving={saving}
          titlePlaceholder={titlePlaceholder}
          submitLabel={submitLabel}
          titleElement={<DialogTitle className="text-[18px] font-bold">{heading}</DialogTitle>}
          cancelElement={
            <DialogClose asChild>
              <Button type="button" variant="outline" className="h-9 min-w-20">
                取消
              </Button>
            </DialogClose>
          }
          onTitleChange={onTitleChange}
          onDueAtChange={onDueAtChange}
          onDeleteImage={onDeleteImage}
          onSubmit={onSubmit}
        />
      </DialogContent>
    </Dialog>
  );
}

export function TodoCreateFormPanel({
  heading = "新建待办",
  todoTitle,
  dueAt,
  images,
  saving = false,
  titlePlaceholder = "待办内容",
  submitLabel = "创建",
  titleElement,
  cancelElement,
  onCancel,
  onTitleChange,
  onDueAtChange,
  onDeleteImage,
  onSubmit,
}: TodoCreateFormPanelProps) {
  return (
    <>
      <DialogHeader className="relative border-b border-border/60 px-5 py-4 pr-12">
        {titleElement ?? <h1 className="text-[18px] font-bold leading-tight tracking-tight">{heading}</h1>}
        {onCancel && (
          <button
            type="button"
            className="absolute right-4 top-4 rounded-md p-1 opacity-60 transition-opacity hover:bg-black/5 hover:opacity-100 focus:outline-none dark:hover:bg-white/10"
            aria-label="关闭"
            onClick={onCancel}
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </DialogHeader>
      <form className="contents" onSubmit={onSubmit}>
        <div className="space-y-4 px-5 pb-4 pt-5">
          <div>
            <DueDateField
              value={dueAt}
              className="h-10 w-full"
              floatingLabel
              popoverPortalled={false}
              onChange={onDueAtChange}
            />
          </div>

          <div>
            <FloatingTextarea
              id="new-todo-title"
              autoFocus
              value={todoTitle}
              maxLength={120}
              placeholder={titlePlaceholder}
              onChange={(event) => onTitleChange(event.target.value)}
            />
          </div>

          <div>
            {images.length > 0 ? (
              <ImageStrip images={images} onDelete={onDeleteImage} />
            ) : (
              <div className="flex h-14 items-center justify-center gap-2 rounded-lg border border-dashed border-border/70 bg-background/46 text-muted-foreground">
                <ImagePlus className="h-4 w-4" />
                <span className="text-[13px]">暂无图片</span>
              </div>
            )}
          </div>
        </div>

        <DialogFooter className="gap-2 border-t border-border/60 bg-foreground/[0.018] px-5 py-4 sm:space-x-0">
          {cancelElement ?? (
            <Button type="button" variant="outline" className="h-9 min-w-20" onClick={onCancel}>
              取消
            </Button>
          )}
          <Button type="submit" className="h-9 min-w-24" disabled={saving}>
            {submitLabel}
          </Button>
        </DialogFooter>
      </form>
    </>
  );
}

export function todoDateTimeLocalToIso(value: string) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

function FloatingTextarea({
  id,
  value,
  placeholder,
  maxLength,
  autoFocus,
  className,
  onChange,
  onPaste,
}: {
  id: string;
  value: string;
  placeholder: string;
  maxLength?: number;
  autoFocus?: boolean;
  className?: string;
  onChange: ChangeEventHandler<HTMLTextAreaElement>;
  onPaste?: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
}) {
  const [focused, setFocused] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const floated = focused || value.length > 0;

  useEffect(() => {
    if (!autoFocus) return;

    const frame = requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;

      const end = textarea.value.length;
      textarea.focus();
      textarea.setSelectionRange(end, end);
    });

    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        id={id}
        autoFocus={autoFocus}
        value={value}
        maxLength={maxLength}
        placeholder={floated ? "" : placeholder}
        className={cn(
          "block min-h-20 w-full resize-none rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 py-3 text-[14px] leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50",
          floated && "border-primary/45",
          className
        )}
        onChange={onChange}
        onPaste={onPaste}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      <span
        className={cn(
          "pointer-events-none absolute left-3 top-px z-10 origin-left -translate-y-1/2 rounded-sm bg-[var(--todo-field-bg)] px-1 text-[11px] font-medium leading-none transition-all duration-150",
          floated
            ? cn("scale-100 opacity-100", focused ? "text-primary" : "text-muted-foreground")
            : "scale-95 opacity-0"
        )}
      >
        {placeholder}
      </span>
    </div>
  );
}

function DueDateField({
  value,
  className,
  floatingLabel = false,
  popoverPortalled = true,
  onChange,
}: {
  value: string;
  className?: string;
  floatingLabel?: boolean;
  popoverPortalled?: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
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
  const floated = floatingLabel && (open || Boolean(value));
  const isBaseDateDisabled = isDueDateDisabled(baseDate);
  const todayDisabled = isDueDateDisabled(new Date());

  return (
    <div className={cn("relative h-10 shrink-0", className)}>
      {floatingLabel && (
        <span
          className={cn(
            "pointer-events-none absolute left-3 top-px z-10 origin-left -translate-y-1/2 rounded-sm bg-[var(--todo-field-bg)] px-1 text-[11px] font-medium leading-none transition-all duration-150",
            floated
              ? cn("scale-100 opacity-100", open ? "text-primary" : "text-muted-foreground")
              : "scale-95 opacity-0"
          )}
        >
          {placeholder}
        </span>
      )}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "flex h-full w-full min-w-0 items-center gap-2 rounded-lg px-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
              floatingLabel
                ? cn(
                    "border border-border/70 bg-[var(--todo-field-bg)] shadow-sm shadow-emerald-950/[0.03] hover:brightness-[1.02]",
                    floated && "border-primary/45"
                  )
                : "glass-subtle hover:bg-white/45 dark:hover:bg-white/8",
              value ? "pr-9" : "pr-3"
            )}
            aria-label={placeholder}
          >
            <CalendarClock className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className={cn("min-w-0 truncate text-[13px] font-medium", value ? "text-foreground" : "text-muted-foreground")}>
              {value ? formatDueFieldValue(value) : floated ? "" : placeholder}
            </span>
          </button>
        </PopoverTrigger>

        {value && (
          <button
            type="button"
            className="absolute right-1.5 top-1/2 z-20 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/8 hover:text-foreground"
            aria-label="清除截止时间"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onChange("");
              setOpen(false);
            }}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}

        <PopoverContent
          side="bottom"
          align="start"
          collisionPadding={12}
          portalled={popoverPortalled}
          className="w-[492px] overflow-hidden p-0"
        >
          <div className="grid grid-cols-[minmax(0,1fr)_132px] gap-3 p-3">
            <Calendar
              className="p-0"
              month={visibleMonth}
              selected={selectedDate}
              isDateDisabled={isDueDateDisabled}
              onMonthChange={setVisibleMonth}
              onSelect={(date) => commit(date, DEFAULT_DUE_HOUR, DEFAULT_DUE_MINUTE)}
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
                        onClick={() => commitAndClose(baseDate, hour, option)}
                      >
                        {option}
                      </button>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>
          <div className="border-t border-border/60 p-3">
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

function ImageStrip({
  images,
  onDelete,
}: {
  images: DraftTodoImage[];
  onDelete: (image: DraftTodoImage) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {images.map((image) => (
        <span key={image.local_id} className="group relative block h-20 w-24 overflow-hidden rounded-lg border border-border/60 bg-foreground/5">
          <img src={image.data_url} alt="" className="h-full w-full object-cover" draggable={false} />
          <button
            type="button"
            className="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md bg-background/85 text-muted-foreground shadow-sm hover:text-rose-600"
            aria-label="删除图片"
            onClick={() => onDelete(image)}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </span>
      ))}
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
