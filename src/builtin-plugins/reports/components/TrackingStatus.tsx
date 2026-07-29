import { cn } from "@/lib/utils";

export function TrackingStatus({ className }: { className?: string }) {
  return (
    <div className={cn("glass-subtle flex h-9 items-center gap-2 rounded-lg px-3", className)}>
      <span className="relative flex h-2 w-2 shrink-0">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-[3px] bg-emerald-400 opacity-60" />
        <span className="relative inline-flex h-2 w-2 rounded-[3px] bg-emerald-400" />
      </span>
      <span className="text-[11px] text-muted-foreground">统计中</span>
    </div>
  );
}
