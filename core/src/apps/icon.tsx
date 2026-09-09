import { useEffect, useState } from "react";
import type { AppIconDescriptor } from "@/apps/types";
import { cn } from "@/lib/utils";

export function AppIconView({
  icon,
  className,
  imgClassName,
}: {
  icon: AppIconDescriptor;
  className?: string;
  imgClassName?: string;
}) {
  const [failed, setFailed] = useState(false);
  const src = icon.type === "file" ? icon.url : undefined;

  useEffect(() => {
    setFailed(false);
  }, [src]);

  if (icon.type === "lucide") {
    const Icon = icon.icon;
    return <Icon className={className} aria-hidden="true" />;
  }

  if (!src || failed) {
    return <span className={cn("inline-block size-4 rounded bg-muted", className)} aria-hidden="true" />;
  }

  return (
    <img
      src={src}
      alt=""
      className={cn("size-4 object-contain", imgClassName, className)}
      draggable={false}
      onError={() => setFailed(true)}
    />
  );
}
