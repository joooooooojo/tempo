import * as React from "react";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

type DataTableProps = React.HTMLAttributes<HTMLDivElement> & {
  children: React.ReactNode;
  empty?: boolean;
  emptyContent?: React.ReactNode;
  footer?: React.ReactNode;
  loading?: boolean;
  loadingContent?: React.ReactNode;
  scrollAreaLabel?: string;
  scrollContentClassName?: string;
};

function DataTable({
  children,
  className,
  empty = false,
  emptyContent,
  footer,
  loading = false,
  loadingContent,
  scrollAreaLabel = "数据表格",
  scrollContentClassName,
  ...props
}: DataTableProps) {
  return (
    <Card
      className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", className)}
      {...props}
    >
      {loading ? (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 px-4 py-12 text-[13px] text-muted-foreground">
          {loadingContent}
        </div>
      ) : empty ? (
        <div className="flex min-h-0 flex-1 items-center justify-center px-4 py-12 text-center text-[13px] text-muted-foreground">
          {emptyContent}
        </div>
      ) : (
        <>
          <ScrollArea
            className="relative min-h-0 flex-1 overflow-hidden"
            scrollbars="both"
            aria-label={scrollAreaLabel}
          >
            <div className={cn("min-w-full", scrollContentClassName)}>{children}</div>
          </ScrollArea>
          {footer && <div className="shrink-0">{footer}</div>}
        </>
      )}
    </Card>
  );
}

export { DataTable };
