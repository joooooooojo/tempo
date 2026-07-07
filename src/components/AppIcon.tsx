import { cn } from "@/lib/utils";

interface AppIconProps {
  name: string;
  iconDataUrl?: string | null;
  className?: string;
  fallbackClassName?: string;
}

export function AppIcon({
  name,
  iconDataUrl,
  className,
  fallbackClassName,
}: AppIconProps) {
  const initial = name.trim().charAt(0).toUpperCase() || "?";

  if (iconDataUrl) {
    return (
      <img
        src={iconDataUrl}
        alt=""
        className={cn("h-9 w-9 shrink-0 rounded-lg object-contain", className)}
        loading="lazy"
      />
    );
  }

  return (
    <span
      className={cn(
        "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white shadow-md",
        fallbackClassName,
        className
      )}
      aria-hidden="true"
    >
      {initial}
    </span>
  );
}
