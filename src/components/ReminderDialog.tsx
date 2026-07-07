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
    case "app_limit_warn":
      return {
        title: "应用时长预警",
        description: `${event.app_name} 今日使用已达 ${event.percent}%，请注意控制使用时间。`,
        action: "知道了",
      };
    case "app_limit_reached":
      return {
        title: "应用限额提醒",
        description: `${event.app_name} 今日使用时长已达到设定上限。你仍可继续使用，请自主决定是否休息。`,
        action: "继续使用",
      };
  }
}
