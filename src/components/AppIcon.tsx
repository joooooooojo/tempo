import { cn, isMacTarget } from "@/lib/utils";

const macOS = isMacTarget;

const sizeStyles = {
  md: {
    slot: "size-11",
    padding: "p-1",
    fallbackText: "text-sm",
  },
  sm: {
    slot: "size-7",
    padding: "p-1",
    fallbackText: "text-[11px]",
  },
  xs: {
    slot: "size-[22px]",
    padding: "p-0.5",
    fallbackText: "text-[10px]",
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
  const { slot, padding, fallbackText } = sizeStyles[size];

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
          "flex shrink-0 items-center justify-center rounded-lg bg-background/60 ring-1 ring-border/60",
          slot,
          padding,
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
