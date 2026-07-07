import FlipNumbers from "react-flip-numbers";
import { cn } from "@/lib/utils";

type FlipNumberProps = {
  value: number;
  size?: "hero" | "compact";
  tone?: "primary" | "muted";
  className?: string;
};

export function FlipNumber({
  value,
  size = "compact",
  tone = "muted",
  className,
}: FlipNumberProps) {
  const safeValue = String(Math.max(0, Math.floor(value)));
  const isHero = size === "hero";

  return (
    <span
      className={cn(
        "flip-number stat-value inline-flex items-end font-bold leading-none",
        tone === "primary" ? "flip-number-primary" : "flip-number-muted",
        className
      )}
      aria-label={safeValue}
    >
      <FlipNumbers
        play
        numbers={safeValue}
        height={isHero ? 48 : 30}
        width={isHero ? 30 : 19}
        color={tone === "primary" ? "hsl(var(--primary))" : "hsl(var(--foreground) / 0.76)"}
        background="transparent"
        perspective={isHero ? 760 : 560}
        duration={0.46}
        delay={0}
        numberClassName="flip-number-digit"
        numberStyle={{
          fontFamily: "var(--font-sans)",
          fontWeight: 800,
          letterSpacing: "0",
          lineHeight: 1,
        }}
      />
    </span>
  );
}
