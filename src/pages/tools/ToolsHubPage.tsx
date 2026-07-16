import { Link } from "react-router-dom";
import { ArrowRight, FileCode2, Languages } from "lucide-react";
import { cn } from "@/lib/utils";

const tools = [
  {
    to: "/tools/hosts",
    title: "Hosts",
    description: "快速编辑、切换多套 hosts 配置。一次授权后即可直接保存。",
    icon: FileCode2,
  },
  {
    to: "/tools/translate",
    title: "聚合翻译",
    description: "对接有道、百度、腾讯、Google、DeepL 等翻译 API，密钥由你本地配置。",
    icon: Languages,
  },
] as const;

export function ToolsHubPage() {
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-6">
      <header className="space-y-1">
        <h1 className="text-xl font-semibold tracking-tight text-foreground">小工具</h1>
        <p className="text-[13px] text-muted-foreground">本地实用工具合集，按需使用，不影响核心功能。</p>
      </header>

      <div className="grid gap-3 sm:grid-cols-2">
        {tools.map(({ to, title, description, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            className={cn(
              "group flex flex-col gap-3 rounded-xl border border-border/60 bg-background/40 p-4",
              "transition-colors hover:border-primary/40 hover:bg-primary/5"
            )}
          >
            <div className="flex items-center justify-between gap-3">
              <span className="flex size-10 items-center justify-center rounded-lg bg-primary/15 text-primary">
                <Icon className="size-5" strokeWidth={1.9} />
              </span>
              <ArrowRight className="size-4 text-muted-foreground opacity-0 transition group-hover:translate-x-0.5 group-hover:opacity-100" />
            </div>
            <div className="space-y-1">
              <h2 className="text-[15px] font-semibold text-foreground">{title}</h2>
              <p className="text-[12px] leading-relaxed text-muted-foreground">{description}</p>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}
