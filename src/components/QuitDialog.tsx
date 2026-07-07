import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

interface QuitDialogProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function QuitDialog({ open, onCancel, onConfirm }: QuitDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(v) => !v && onCancel()}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>确认退出</DialogTitle>
          <DialogDescription>
            退出后统计服务将停止，历史数据已自动保存。确定要退出吗？
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={onCancel}>
            取消
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            退出软件
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
