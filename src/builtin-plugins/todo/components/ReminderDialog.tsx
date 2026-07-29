import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { ReminderEvent } from "@/types";

interface ReminderDialogProps {
  event: ReminderEvent | null;
  onDismiss: () => void;
}

export function ReminderDialog({ event, onDismiss }: ReminderDialogProps) {
  if (!event) return null;

  const config = getConfig(event);

  return (
    <Dialog open onOpenChange={(open) => !open && onDismiss()}>
      <DialogPanel className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{config.title}</DialogTitle>
        </DialogHeader>
        <DialogContent className="flex-none py-4">
          <DialogDescription>{config.description}</DialogDescription>
        </DialogContent>
        <DialogFooter>
          <Button onClick={onDismiss}>{config.action}</Button>
        </DialogFooter>
      </DialogPanel>
    </Dialog>
  );
}

function getConfig(event: ReminderEvent) {
  switch (event.type) {
    case "todo_due":
      return getTodoDueConfig(event);
  }
}

function getTodoDueConfig(event: {
  title: string;
  lead: "1d" | "1h" | "due" | "custom";
  hours?: number;
}) {
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
