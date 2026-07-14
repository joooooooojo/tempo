import { useState, type MouseEvent, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Copy, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

export function isSafeLinkHref(href: string) {
  if (/^mailto:/i.test(href)) return true;
  try {
    const url = new URL(href);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function LinkActionPopover({
  href,
  children,
  className,
}: {
  href: string;
  children: ReactNode;
  className?: string;
}) {
  const [open, setOpen] = useState(false);

  if (!isSafeLinkHref(href)) {
    return <span className={className}>{children}</span>;
  }

  const stop = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };

  const handleOpen = async () => {
    try {
      await openUrl(href);
      setOpen(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "无法打开链接");
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(href);
      toast.success("已复制链接");
      setOpen(false);
    } catch {
      toast.error("复制失败");
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "inline break-all text-left text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary",
            className
          )}
          onClick={stop}
        >
          {children}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-72 space-y-3 p-3"
        onClick={stop}
      >
        <p className="break-all text-[12px] text-muted-foreground" title={href}>
          {href}
        </p>
        <div className="flex gap-2">
          <Button type="button" size="sm" className="flex-1 gap-1.5" onClick={() => void handleOpen()}>
            <ExternalLink className="h-3.5 w-3.5" />
            系统浏览器打开
          </Button>
          <Button type="button" size="sm" variant="outline" className="gap-1.5" onClick={() => void handleCopy()}>
            <Copy className="h-3.5 w-3.5" />
            复制
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
