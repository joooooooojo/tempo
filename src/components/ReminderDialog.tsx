import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { ReminderEvent } from "@/types";

type DialogReminderEvent = Exclude<ReminderEvent, { type: "eye_care" }>;

interface ReminderDialogProps {
  event: ReminderEvent | null;
  onDismiss: () => void;
}

export function ReminderDialog({ event, onDismiss }: ReminderDialogProps) {
  if (!event || event.type === "eye_care") return null;

  const config = getConfig(event);

  return (
    <Dialog open onOpenChange={(open) => !open && onDismiss()}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{config.title}</DialogTitle>
          <DialogDescription>{config.description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button onClick={onDismiss}>{config.action}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function getConfig(event: DialogReminderEvent) {
  switch (event.type) {
    case "night":
      return {
        title: "夜间作息提醒",
        description: "已经进入夜间时段，建议减少电子设备使用，保证充足睡眠。",
        action: "好的",
      };
    case "pomodoro_phase_end":
      return getPomodoroConfig(event);
    case "todo_due":
      return getTodoDueConfig(event);
  }
}

function getTodoDueConfig(event: { title: string; lead: "1d" | "1h" | "due" | "custom"; hours?: number }) {
  if (event.lead === "1d") {
    return {
      title: "待办即将截止",
      description: `「${event.title}」将在 1 天后截止，记得处理。`,
      action: "知道了",
    };
  }
  if (event.lead === "1h") {
    return {
      title: "待办即将截止",
      description: `「${event.title}」将在 1 小时后截止，请尽快完成。`,
      action: "知道了",
    };
  }
  if (event.lead === "custom" && event.hours) {
    return {
      title: "待办即将截止",
      description: `「${event.title}」将在 ${event.hours} 小时后截止，请尽快完成。`,
      action: "知道了",
    };
  }
  return {
    title: "待办已到截止时间",
    description: `「${event.title}」已到截止时间。`,
    action: "知道了",
  };
}

function getPomodoroConfig(event: { phase: "work" | "short_break" | "long_break"; skipped: boolean }) {
  switch (event.phase) {
    case "work":
      return {
        title: event.skipped ? "专注已跳过" : "专注完成",
        description: event.skipped
          ? "已进入休息阶段，放松一下再继续。"
          : "太棒了！休息一下，活动活动身体。",
        action: "好的",
      };
    case "short_break":
      return {
        title: event.skipped ? "短休已跳过" : "短休结束",
        description: event.skipped
          ? "已开始新一轮专注，加油！"
          : "休息够了，准备好开始下一轮专注了吗？",
        action: "继续",
      };
    case "long_break":
      return {
        title: event.skipped ? "长休已跳过" : "长休结束",
        description: event.skipped
          ? "新一轮专注已开始，保持好节奏。"
          : "长休结束，新一轮番茄循环开始！",
        action: "开始专注",
      };
  }
}
