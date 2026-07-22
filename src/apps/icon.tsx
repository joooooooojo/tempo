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
  if (icon.type === "lucide") {
    const Icon = icon.icon;
    return <Icon className={className} aria-hidden="true" />;
  }

  const src = icon.url;
  if (!src) {
    return <span className={cn("inline-block size-4 rounded bg-muted", className)} aria-hidden="true" />;
  }

  return (
    <img
      src={src}
      alt=""
      className={cn("size-4 object-contain", imgClassName, className)}
      draggable={false}
    />
  );
}
