import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function WorkspaceKeepAlive({
  active,
  children,
  className,
}: {
  active: boolean;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-content",
        !active && "plugin-dev-content--inactive",
        className,
      )}
      hidden={!active}
      aria-hidden={!active}
      inert={!active ? true : undefined}
    >
      {children}
    </div>
  );
}
