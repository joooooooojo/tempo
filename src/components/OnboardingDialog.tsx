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
import { Clock3, Shield } from "lucide-react";

interface OnboardingDialogProps {
  open: boolean;
  onComplete: () => void;
}

export function OnboardingDialog({ open, onComplete }: OnboardingDialogProps) {
  return (
    <Dialog open={open}>
      <DialogPanel className="sm:max-w-md">
        <DialogHeader showCloseButton={false} className="items-center text-center">
          <div className="mb-1 flex size-14 items-center justify-center rounded-xl bg-gradient-to-br from-emerald-300 to-teal-500 shadow-lg shadow-emerald-500/25">
            <Clock3 className="size-6 text-white" strokeWidth={1.9} />
          </div>
          <DialogTitle className="text-lg">欢迎使用 Tempo</DialogTitle>
        </DialogHeader>
        <DialogContent className="flex-none">
          <DialogDescription asChild>
            <div className="flex flex-col gap-3">
              <p className="text-center text-[13px]">
                后台静默统计屏幕与应用使用时长，帮助你更好地管理数字生活。
              </p>
              <div className="glass-subtle rounded-lg p-4">
                <p className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
                  <Shield className="size-4 text-primary" />
                  权限与隐私
                </p>
                <ul className="mt-2 flex flex-col gap-1.5 text-[12px] text-muted-foreground">
                  <li>· 读取前台应用名称以统计时长</li>
                  <li>· 全程离线，不上传任何数据</li>
                  <li>· 不读取文档、不截屏</li>
                </ul>
              </div>
            </div>
          </DialogDescription>
        </DialogContent>
        <DialogFooter>
          <Button className="w-full sm:w-full" onClick={onComplete}>
            开始使用
          </Button>
        </DialogFooter>
      </DialogPanel>
    </Dialog>
  );
}
