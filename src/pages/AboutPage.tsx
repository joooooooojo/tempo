import { Clock3, Shield, WifiOff, HardDrive } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";

export function AboutPage() {
  return (
    <div className="mx-auto max-w-md space-y-5">
      <div className="flex flex-col items-center py-6">
        <div className="relative">
          <div className="absolute inset-0 rounded-xl bg-gradient-to-br from-emerald-300 to-teal-500 blur-xl opacity-35" />
          <div className="relative flex h-20 w-20 items-center justify-center rounded-xl bg-gradient-to-br from-emerald-300 to-teal-500 shadow-2xl shadow-emerald-500/25">
            <Clock3 className="h-9 w-9 text-white" strokeWidth={1.8} />
          </div>
        </div>
        <h2 className="mt-5 text-2xl font-extrabold tracking-tight">时窗</h2>
        <p className="mt-1 text-[13px] text-muted-foreground">Version 1.0 · 离线桌面工具</p>
      </div>

      <Card className="overflow-hidden">
        <Feature icon={WifiOff} title="完全离线" desc="零网络请求，零数据上传" />
        <Feature icon={Shield} title="隐私优先" desc="仅统计应用名与时长" />
        <Feature icon={HardDrive} title="本地存储" desc="SQLite · 30 天历史" />
      </Card>

      <Card>
        <CardContent className="p-5">
          <p className="text-[13px] font-semibold">隐私声明</p>
          <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
            不读取文档、不截屏、不收集内容数据。所有统计保存在本地，卸载时可清除。
          </p>
        </CardContent>
      </Card>
    </div>
  );
}

function Feature({ icon: Icon, title, desc }: { icon: typeof WifiOff; title: string; desc: string }) {
  return (
    <div className="list-row">
      <div className="flex items-center gap-3">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10">
          <Icon className="h-4 w-4 text-primary" strokeWidth={2} />
        </div>
        <div>
          <p className="text-[14px] font-semibold">{title}</p>
          <p className="text-[12px] text-muted-foreground">{desc}</p>
        </div>
      </div>
    </div>
  );
}
