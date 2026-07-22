import { useEffect, useState } from "react";
import { AppWindow } from "lucide-react";
import { cn } from "@/lib/utils";

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
  fallback?: "initial" | "application";
}

export function AppIcon({
  name,
  iconDataUrl,
  className,
  fallbackClassName,
  size = "md",
  fallback = "initial",
}: AppIconProps) {
  const [imageFailed, setImageFailed] = useState(false);
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  const { slot, padding, fallbackText } = sizeStyles[size];

  useEffect(() => {
    setImageFailed(false);
  }, [iconDataUrl]);

  const fallbackContent =
    fallback === "application" ? <AppWindow className="size-4" aria-hidden="true" /> : initial;
  const showImage = Boolean(iconDataUrl && !imageFailed);

  return (
    <span
      data-slot="app-icon"
      className={cn(
        "flex shrink-0 items-center justify-center overflow-hidden rounded-[38.9%] bg-background/60 text-muted-foreground shadow-sm ring-1 ring-border/60",
        slot,
        padding,
        !showImage && "font-bold",
        !showImage && fallback === "initial" && fallbackText,
        !showImage && fallbackClassName,
        className
      )}
      aria-hidden="true"
    >
      {showImage ? (
        <img
          src={iconDataUrl ?? undefined}
          alt=""
          className="size-full rounded-[38.9%] object-contain"
          loading="lazy"
          onError={() => setImageFailed(true)}
        />
      ) : (
        fallbackContent
      )}
    </span>
  );
}
