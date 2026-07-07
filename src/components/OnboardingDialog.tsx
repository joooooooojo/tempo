import {
  Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
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
      <DialogContent onPointerDownOutside={(e) => e.preventDefault()}>
        <DialogHeader>
          <div className="mx-auto mb-3 flex h-14 w-14 items-center justify-center rounded-xl bg-gradient-to-br from-emerald-300 to-teal-500 shadow-lg shadow-emerald-500/25">
            <Clock3 className="h-6 w-6 text-white" strokeWidth={1.9} />
          </div>
          <DialogTitle className="text-center text-lg">欢迎使用时窗</DialogTitle>
          <DialogDescription asChild>
            <div className="space-y-3 pt-1">
              <p className="text-center text-[13px]">
                后台静默统计屏幕与应用使用时长，帮助你更好地管理数字生活。
              </p>
              <div className="glass-subtle rounded-lg p-4">
                <p className="flex items-center gap-2 text-[13px] font-semibold">
                  <Shield className="h-4 w-4 text-primary" />
                  权限与隐私
                </p>
                <ul className="mt-2 space-y-1.5 text-[12px] text-muted-foreground">
                  <li>· 读取前台应用名称以统计时长</li>
                  <li>· 全程离线，不上传任何数据</li>
                  <li>· 不读取文档、不截屏</li>
                </ul>
              </div>
            </div>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="mt-2">
          <Button className="w-full" onClick={onComplete}>开始使用</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
