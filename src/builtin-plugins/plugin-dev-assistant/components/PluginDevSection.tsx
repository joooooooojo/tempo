import type { ReactNode } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function PluginDevSection({
  title,
  description,
  action,
  children,
  className,
  contentClassName,
}: {
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
}) {
  const hasHeader = title != null || description != null || action != null;

  return (
    <Card size="sm" className={cn("plugin-dev-card", className)}>
      {hasHeader ? (
        <CardHeader className="border-b">
          <div className="min-w-0">
            {title != null ? <CardTitle>{title}</CardTitle> : null}
            {description != null ? (
              <CardDescription>{description}</CardDescription>
            ) : null}
          </div>
          {action != null ? <CardAction>{action}</CardAction> : null}
        </CardHeader>
      ) : null}
      <CardContent className={cn("flex flex-col gap-4", contentClassName)}>
        {children}
      </CardContent>
    </Card>
  );
}
