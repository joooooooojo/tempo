import {
  type ClipboardEvent,
  type FormEvent,
  type ReactNode,
} from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import type { TodoImageInput } from "@/lib/api";
import { cn } from "@/lib/utils";
import {
  clipboardHasImages,
  insertTextAtSelection,
  markdownImagesFromClipboard,
} from "@/lib/markdownImages";
import type { TodoRecurrence } from "@/types";
import { TodoSubtaskDraftList } from "@/builtin-plugins/todo/components/TodoSubtasks";
import {
  FloatingInput,
  FloatingTextarea,
} from "@/builtin-plugins/todo/components/todoCreateFields";
import { MoreSettingsDialog } from "@/builtin-plugins/todo/components/TodoMoreSettings";

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
  tags?: string[];
  tagSuggestions?: string[];
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
  onTagsChange?: (value: string[]) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

type TodoCreateFormPanelProps = Omit<TodoCreateDialogProps, "open" | "onOpenChange"> & {
  titleElement?: ReactNode;
  cancelElement?: ReactNode;
  layout?: "dialog" | "window";
  onCancel?: () => void;
};

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
  tags = [],
  tagSuggestions = [],
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
  onTagsChange,
  onSubmit,
}: TodoCreateDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPanel className="todo-create-dialog max-h-[min(680px,calc(100vh-2rem))] w-[calc(100vw-2rem)] max-w-[680px] sm:max-w-[680px]">
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
          tags={tags}
          tagSuggestions={tagSuggestions}
          saving={saving}
          titlePlaceholder={titlePlaceholder}
          contentPlaceholder={contentPlaceholder}
          submitLabel={submitLabel}
          bodyExtra={bodyExtra}
          titleElement={<DialogTitle className="text-[20px] font-bold">{heading}</DialogTitle>}
          cancelElement={
            <DialogClose asChild>
              <Button type="button" variant="outline" className="h-10 min-w-24">
                取消
              </Button>
            </DialogClose>
          }
          onCancel={() => onOpenChange(false)}
          onTitleChange={onTitleChange}
          onContentChange={onContentChange}
          onDueAtChange={onDueAtChange}
          onRecurrenceChange={onRecurrenceChange}
          onRemind1dChange={onRemind1dChange}
          onRemind1hChange={onRemind1hChange}
          onRemindCustomHoursChange={onRemindCustomHoursChange}
          onSubtasksChange={onSubtasksChange}
          onTagsChange={onTagsChange}
          onSubmit={onSubmit}
        />
      </DialogPanel>
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
  tags = [],
  tagSuggestions = [],
  saving = false,
  titlePlaceholder = "标题",
  contentPlaceholder = "内容（支持 Markdown，粘贴图片会嵌入正文）",
  submitLabel = "创建",
  titleElement,
  cancelElement,
  bodyExtra,
  layout = "dialog",
  onCancel,
  onTitleChange,
  onContentChange,
  onDueAtChange,
  onRecurrenceChange,
  onRemind1dChange,
  onRemind1hChange,
  onRemindCustomHoursChange,
  onSubtasksChange,
  onTagsChange,
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
        showCloseButton={!isWindowLayout}
        className={cn(
          isWindowLayout && "relative flex-row items-center justify-between px-5 select-none"
        )}
      >
        {titleElement ?? (
          <h1
            data-tauri-drag-region={isWindowLayout ? "" : undefined}
            className="flex min-h-7 items-center text-[15px] font-semibold leading-none tracking-tight"
          >
            {heading}
          </h1>
        )}
        {isWindowLayout && onCancel && (
          <Button
            data-no-drag
            variant="ghost"
            size="icon-sm"
            aria-label="关闭"
            onClick={onCancel}
          >
            <X className="relative size-3.5" />
          </Button>
        )}
      </DialogHeader>
      <form className="flex min-h-0 flex-1 flex-col overflow-hidden" autoComplete="off" onSubmit={onSubmit}>
        <DialogContent
          className={cn(
            "no-scrollbar flex flex-col gap-4",
            isWindowLayout && "gap-3.5 px-5"
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
              className={cn(isWindowLayout ? "h-full min-h-32" : "min-h-44")}
              onChange={(event) => onContentChange(event.target.value)}
              onPaste={handleContentPaste}
            />
          </div>

          {onSubtasksChange && (
            <TodoSubtaskDraftList items={subtasks} onChange={onSubtasksChange} />
          )}
          {bodyExtra}
        </DialogContent>

        <DialogFooter className={cn("sm:justify-between", isWindowLayout && "px-5")}>
          <MoreSettingsDialog
            tags={tags}
            tagSuggestions={tagSuggestions}
            recurrence={recurrence}
            dueAt={dueAt}
            remind1d={remind1d}
            remind1h={remind1h}
            remindCustomHours={remindCustomHours}
            onTagsChange={onTagsChange}
            onRecurrenceChange={onRecurrenceChange}
            onDueAtChange={onDueAtChange}
            onRemind1dChange={onRemind1dChange}
            onRemind1hChange={onRemind1hChange}
            onRemindCustomHoursChange={onRemindCustomHoursChange}
          />
          <div className="ml-auto flex shrink-0 items-center gap-3">
            {cancelElement ?? (
              <Button type="button" variant="outline" className="h-10 min-w-24" onClick={onCancel}>
                取消
              </Button>
            )}
            <Button type="submit" className="h-10 min-w-28" disabled={saving || !todoTitle.trim()}>
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
