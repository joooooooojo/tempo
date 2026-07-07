import { cn, isMacOS } from "@/lib/utils";

const macOS = isMacOS();

const sizeStyles = {
  md: {
    slot: "h-11 w-11",
    fallbackText: "text-sm",
  },
  sm: {
    slot: "h-7 w-7",
    fallbackText: "text-[11px]",
  },
} as const;

interface AppIconProps {
  name: string;
  iconDataUrl?: string | null;
  className?: string;
  fallbackClassName?: string;
  size?: keyof typeof sizeStyles;
}

export function AppIcon({
  name,
  iconDataUrl,
  className,
  fallbackClassName,
  size = "md",
}: AppIconProps) {
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  const { slot, fallbackText } = sizeStyles[size];

  if (iconDataUrl) {
    if (macOS) {
      return (
        <img
          src={iconDataUrl}
          alt=""
          className={cn("shrink-0 object-contain", slot, className)}
          loading="lazy"
        />
      );
    }

    return (
      <span
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg bg-background/60 p-1 ring-1 ring-border/60",
          slot,
          className
        )}
      >
        <img
          src={iconDataUrl}
          alt=""
          className="h-full w-full rounded-md object-contain"
          loading="lazy"
        />
      </span>
    );
  }

  if (macOS) {
    return (
      <span
        className={cn(
          "flex shrink-0 items-center justify-center rounded-full font-bold text-white shadow-md",
          slot,
          fallbackText,
          fallbackClassName,
          className
        )}
        aria-hidden="true"
      >
        {initial}
      </span>
    );
  }

  return (
    <span
      className={cn(
        "flex shrink-0 items-center justify-center rounded-lg font-bold text-white shadow-md",
        slot,
        fallbackText,
        fallbackClassName,
        className
      )}
      aria-hidden="true"
    >
      {initial}
    </span>
  );
}
