import {
  useEffect,
  useRef,
  useState,
  type ChangeEventHandler,
  type ClipboardEvent,
  type FormEvent,
  type ReactNode,
} from "react";
import { CalendarClock, Repeat, X } from "lucide-react";
import { toast } from "sonner";
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
import { recurrenceOptions, recurrenceLabel } from "@/lib/todoMeta";
import { clipboardHasImages, insertTextAtSelection, markdownImagesFromClipboard } from "@/lib/markdownImages";
import { cn } from "@/lib/utils";
import type { TodoRecurrence } from "@/types";
import { TodoSubtaskDraftList } from "@/components/todos/TodoSubtasks";

export interface DraftTodoImage extends TodoImageInput {
  local_id: string;
}

type TodoCreateDialogProps = {
  open: boolean;
  heading?: string;
  todoTitle: string;
  todoContent: string;
  dueAt: string;
  recurrence?: TodoRecurrence;
  remind1d?: boolean;
  remind1h?: boolean;
  remindCustomHours?: number | null;
  subtasks?: string[];
  saving?: boolean;
  titlePlaceholder?: string;
  contentPlaceholder?: string;
  submitLabel?: string;
  bodyExtra?: ReactNode;
  onOpenChange: (open: boolean) => void;
  onTitleChange: (value: string) => void;
  onContentChange: (value: string) => void;
  onDueAtChange: (value: string) => void;
  onRecurrenceChange?: (value: TodoRecurrence) => void;
  onRemind1dChange?: (value: boolean) => void;
  onRemind1hChange?: (value: boolean) => void;
  onRemindCustomHoursChange?: (value: number | null) => void;
  onSubtasksChange?: (value: string[]) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

type TodoCreateFormPanelProps = Omit<TodoCreateDialogProps, "open" | "onOpenChange"> & {
  titleElement?: ReactNode;
  cancelElement?: ReactNode;
  layout?: "dialog" | "window";
  popoverContainer?: HTMLElement | null;
  onCancel?: () => void;
};

const DEFAULT_DUE_HOUR = "18";
const DEFAULT_DUE_MINUTE = "00";
const hourOptions = ["08", "09", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "22", "23"];
const minuteOptions = ["00", "15", "30", "45"];

export function TodoCreateDialog({
  open,
  heading = "新建待办事项",
  todoTitle,
  todoContent,
  dueAt,
  recurrence = "none",
  remind1d = false,
  remind1h = false,
  remindCustomHours = null,
  subtasks = [],
  saving = false,
  titlePlaceholder = "标题",
  contentPlaceholder = "内容（支持 Markdown，粘贴图片会嵌入正文）",
  submitLabel = "创建",
  bodyExtra,
  onOpenChange,
  onTitleChange,
  onContentChange,
  onDueAtChange,
  onRecurrenceChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
  onSubtasksChange,
  onSubmit,
}: TodoCreateDialogProps) {
  const [popoverContainer, setPopoverContainer] = useState<HTMLDivElement | null>(null);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        ref={setPopoverContainer}
        className="todo-create-dialog !flex max-h-[520px] max-w-[520px] flex-col gap-0 overflow-visible rounded-xl border-border/80 p-0"
        onFocusOutside={(event) => {
          if (isPopoverLayerTarget(event.target)) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (isPopoverLayerTarget(event.target)) event.preventDefault();
        }}
        onPointerDownOutside={(event) => {
          if (isPopoverLayerTarget(event.target)) event.preventDefault();
        }}
      >
        <TodoCreateFormPanel
          heading={heading}
          todoTitle={todoTitle}
          todoContent={todoContent}
          dueAt={dueAt}
          recurrence={recurrence}
          remind1d={remind1d}
          remind1h={remind1h}
          remindCustomHours={remindCustomHours}
          subtasks={subtasks}
          saving={saving}
          titlePlaceholder={titlePlaceholder}
          contentPlaceholder={contentPlaceholder}
          submitLabel={submitLabel}
          bodyExtra={bodyExtra}
          popoverContainer={popoverContainer}
          titleElement={<DialogTitle className="text-[18px] font-bold">{heading}</DialogTitle>}
          cancelElement={
            <DialogClose asChild>
              <Button type="button" variant="outline" className="h-9 min-w-20">
                取消
              </Button>
            </DialogClose>
          }
          onTitleChange={onTitleChange}
          onContentChange={onContentChange}
          onDueAtChange={onDueAtChange}
          onRecurrenceChange={onRecurrenceChange}
          onRemind1dChange={onRemind1dChange}
          onRemind1hChange={onRemind1hChange}
          onRemindCustomHoursChange={onRemindCustomHoursChange}
          onSubtasksChange={onSubtasksChange}
          onSubmit={onSubmit}
        />
      </DialogContent>
    </Dialog>
  );
}

export function TodoCreateFormPanel({
  heading = "新建待办事项",
  todoTitle,
  todoContent,
  dueAt,
  recurrence = "none",
  remind1d = false,
  remind1h = false,
  remindCustomHours = null,
  subtasks = [],
  saving = false,
  titlePlaceholder = "标题",
  contentPlaceholder = "内容（支持 Markdown，粘贴图片会嵌入正文）",
  submitLabel = "创建",
  titleElement,
  cancelElement,
  bodyExtra,
  layout = "dialog",
  popoverContainer,
  onCancel,
  onTitleChange,
  onContentChange,
  onDueAtChange,
  onRecurrenceChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
  onSubtasksChange,
  onSubmit,
}: TodoCreateFormPanelProps) {
  const isWindowLayout = layout === "window";
  const handleContentPaste = async (event: ClipboardEvent<HTMLTextAreaElement>) => {
    if (!clipboardHasImages(event)) return;

    event.preventDefault();
    event.stopPropagation();

    const textarea = event.currentTarget;
    const selectionStart = textarea.selectionStart;
    const selectionEnd = textarea.selectionEnd;
    const { markdown, errors } = await markdownImagesFromClipboard(event);

    for (const error of errors) toast.error(error);
    if (!markdown) return;

    onContentChange(insertTextAtSelection(todoContent, markdown, selectionStart, selectionEnd));
    toast.success("图片已嵌入到 Markdown 内容");
  };

  return (
    <>
      <DialogHeader
        data-tauri-drag-region={isWindowLayout ? "" : undefined}
        className={cn(
          "relative shrink-0 border-b border-border/60 px-5 py-4 pr-12",
          isWindowLayout && "select-none"
        )}
      >
        {titleElement ?? (
          <h1
            data-tauri-drag-region={isWindowLayout ? "" : undefined}
            className="text-[18px] font-bold leading-tight tracking-tight"
          >
            {heading}
          </h1>
        )}
        {onCancel && (
          <button
            data-no-drag
            type="button"
            className="absolute right-4 top-4 rounded-md p-1 opacity-60 transition-opacity hover:bg-black/5 hover:opacity-100 focus:outline-none dark:hover:bg-white/10"
            aria-label="关闭"
            onClick={onCancel}
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </DialogHeader>
      <form className="flex min-h-0 flex-1 flex-col overflow-hidden" onSubmit={onSubmit}>
        <div
          className={cn(
            "no-scrollbar min-h-0 flex-1 overflow-y-auto px-5 pb-4 pt-5",
            isWindowLayout ? "flex flex-col gap-3.5" : "space-y-4"
          )}
        >
          <div className={cn(isWindowLayout && "shrink-0")}>
            <FloatingInput
              id="new-todo-title"
              autoFocus
              required
              value={todoTitle}
              maxLength={120}
              placeholder={titlePlaceholder}
              onChange={(event) => onTitleChange(event.target.value)}
            />
          </div>

          <div className={cn(isWindowLayout && "min-h-0 flex-1")}>
            <FloatingTextarea
              id="new-todo-content"
              value={todoContent}
              placeholder={contentPlaceholder}
              className={cn(isWindowLayout ? "h-full min-h-32" : "min-h-40")}
              onChange={(event) => onContentChange(event.target.value)}
              onPaste={handleContentPaste}
            />
          </div>

          {onSubtasksChange && (
            <TodoSubtaskDraftList items={subtasks} onChange={onSubtasksChange} />
          )}
          {bodyExtra}
        </div>

        <DialogFooter className="shrink-0 flex-row items-center justify-between gap-2 border-t border-border/60 bg-foreground/[0.018] px-5 py-4 sm:space-x-0">
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
            {onRecurrenceChange && (
              <RecurrenceField
                value={recurrence}
                popoverContainer={popoverContainer}
                popoverPortalled={!isWindowLayout}
                popoverSide="top"
                onChange={onRecurrenceChange}
              />
            )}
            {recurrence === "none" && (
              <DueDateField
                value={dueAt}
                className="h-9 min-w-0 flex-1"
                bordered
                compact
                remind1d={remind1d}
                remind1h={remind1h}
                remindCustomHours={remindCustomHours}
                popoverContainer={popoverContainer}
                popoverPortalled={!isWindowLayout}
                popoverSide="top"
                onChange={onDueAtChange}
                onRemind1dChange={onRemind1dChange}
                onRemind1hChange={onRemind1hChange}
                onRemindCustomHoursChange={onRemindCustomHoursChange}
              />
            )}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {cancelElement ?? (
              <Button type="button" variant="outline" className="h-9 min-w-20" onClick={onCancel}>
                取消
              </Button>
            )}
            <Button type="submit" className="h-9 min-w-24" disabled={saving || !todoTitle.trim()}>
              {submitLabel}
            </Button>
          </div>
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

function isPopoverLayerTarget(target: EventTarget | null) {
  return (
    target instanceof Element &&
    Boolean(
      target.closest(
        "[data-radix-popover-content], [data-radix-popper-content-wrapper], [data-radix-focus-guard]"
      )
    )
  );
}

function FloatingInput({
  id,
  value,
  placeholder,
  maxLength,
  autoFocus,
  required,
  className,
  onChange,
}: {
  id: string;
  value: string;
  placeholder: string;
  maxLength?: number;
  autoFocus?: boolean;
  required?: boolean;
  className?: string;
  onChange: ChangeEventHandler<HTMLInputElement>;
}) {
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const floated = focused || value.length > 0;

  useEffect(() => {
    if (!autoFocus) return;

    const frame = requestAnimationFrame(() => {
      const input = inputRef.current;
      if (!input) return;

      const end = input.value.length;
      input.focus();
      input.setSelectionRange(end, end);
    });

    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);

  return (
    <div className="relative">
      <input
        ref={inputRef}
        id={id}
        autoFocus={autoFocus}
        required={required}
        value={value}
        maxLength={maxLength}
        placeholder={floated ? "" : placeholder}
        className={cn(
          "block h-11 w-full rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[14px] font-semibold leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50",
          floated && "border-primary/45",
          className
        )}
        onChange={onChange}
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

function RecurrenceField({
  value,
  className,
  popoverPortalled = true,
  popoverContainer,
  popoverSide = "bottom",
  onChange,
}: {
  value: TodoRecurrence;
  className?: string;
  popoverPortalled?: boolean;
  popoverContainer?: HTMLElement | null;
  popoverSide?: "top" | "bottom";
  onChange: (value: TodoRecurrence) => void;
}) {
  const [open, setOpen] = useState(false);
  const [focused, setFocused] = useState(false);
  const inDialog = Boolean(popoverContainer);
  const active = open || focused || value !== "none";
  const label = recurrenceLabel(value);

  return (
    <div className={cn("relative min-h-9 shrink-0", className)}>
      <Popover modal={inDialog} open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "flex h-9 min-w-[108px] items-center gap-2 rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-left text-[14px] font-semibold shadow-sm shadow-emerald-950/[0.03] transition-colors hover:brightness-[1.02] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
              active && "border-primary/45"
            )}
            aria-label="重复"
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
          >
            <Repeat className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className={cn("truncate", value !== "none" ? "text-foreground" : "text-muted-foreground")}>
              {label}
            </span>
          </button>
        </PopoverTrigger>
        <PopoverContent
          side={popoverSide}
          align="start"
          collisionPadding={12}
          portalled={popoverPortalled}
          container={popoverContainer}
          onOpenAutoFocus={(event) => event.preventDefault()}
          className={cn("w-44 p-1.5", inDialog ? "z-[60]" : popoverPortalled && "z-[120]")}
        >
          {recurrenceOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              className={cn(
                "flex h-9 w-full items-center rounded-md px-2.5 text-left text-[13px] transition-colors",
                option.value === value
                  ? "bg-primary/10 font-semibold text-primary"
                  : "text-foreground hover:bg-foreground/6"
              )}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
              }}
            >
              {option.label}
            </button>
          ))}
        </PopoverContent>
      </Popover>
    </div>
  );
}

function DueDateField({
  value,
  className,
  floatingLabel = false,
  bordered = false,
  compact = false,
  remind1d = false,
  remind1h = false,
  remindCustomHours = null,
  popoverPortalled = true,
  popoverContainer,
  popoverSide = "bottom",
  onChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
}: {
  value: string;
  className?: string;
  floatingLabel?: boolean;
  bordered?: boolean;
  compact?: boolean;
  remind1d?: boolean;
  remind1h?: boolean;
  remindCustomHours?: number | null;
  popoverPortalled?: boolean;
  popoverContainer?: HTMLElement | null;
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
  const floated = floatingLabel && active;
  const useBorderedStyle = floatingLabel || bordered;
  const isBaseDateDisabled = isDueDateDisabled(baseDate);
  const todayDisabled = isDueDateDisabled(new Date());
  const inDialog = Boolean(popoverContainer);
  const hasReminderOptions = Boolean(
    value && (onRemind1dChange || onRemind1hChange || onRemindCustomHoursChange)
  );
  const reminderSummary = formatDueReminderSummary(remind1d, remind1h, remindCustomHours);
  const hasReminder = Boolean(remind1d || remind1h || remindCustomHours);

  return (
    <div className={cn("relative shrink-0", compact ? "h-9" : "min-h-10", className)}>
      {floatingLabel && (
        <span
          className={cn(
            "pointer-events-none absolute left-3 top-px z-10 origin-left -translate-y-1/2 rounded-sm bg-[var(--todo-field-bg)] px-1 text-[11px] font-medium leading-none transition-all duration-150",
            floated
              ? cn("scale-100 opacity-100", open || focused ? "text-primary" : "text-muted-foreground")
              : "scale-95 opacity-0"
          )}
        >
          {placeholder}
        </span>
      )}
      <Popover modal={inDialog} open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "flex h-9 w-full min-w-0 items-center gap-2 rounded-lg px-3 text-left text-[14px] font-semibold leading-5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30",
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
            <span className="relative shrink-0">
              <CalendarClock
                className={cn(
                  "h-4 w-4 transition-colors",
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
            {compact ? (
              <span className="min-w-0 truncate">
                <span className={cn(value ? "text-foreground" : "text-muted-foreground")}>
                  {value ? formatDueFieldValue(value) : floated ? "" : placeholder}
                </span>
              </span>
            ) : (
              <span className="flex min-w-0 flex-col">
                <span className={cn("truncate", value ? "text-foreground" : "text-muted-foreground")}>
                  {value ? formatDueFieldValue(value) : floated ? "" : placeholder}
                </span>
                {value && reminderSummary && (
                  <span className="truncate text-[11px] font-normal text-muted-foreground">{reminderSummary}</span>
                )}
              </span>
            )}
          </button>
        </PopoverTrigger>

        <PopoverContent
          side={popoverSide}
          align="start"
          collisionPadding={12}
          portalled={popoverPortalled}
          container={popoverContainer}
          onOpenAutoFocus={(event) => event.preventDefault()}
          className={cn(
            "w-[min(480px,calc(100vw-2.5rem))] overflow-hidden p-0",
            inDialog ? "z-[60]" : popoverPortalled && "z-[120]"
          )}
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
            <div className="border-t border-border/60 px-3 py-3">
              <span className="mb-2 block text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                提醒
              </span>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-1 py-1">
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
