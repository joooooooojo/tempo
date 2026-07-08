import { useEffect, useRef, useState, type CSSProperties } from "react";
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
  const safeValue = String(Math.max(0, Math.floor(value))).padStart(2, "0");
  const isHero = size === "hero";
  const height = isHero ? 48 : 30;
  const width = isHero ? 30 : 19;
  const color = tone === "primary" ? "hsl(var(--primary))" : "hsl(var(--foreground) / 0.76)";
  const duration = isHero ? 460 : 400;
  const digits = safeValue.split("");

  return (
    <span
      className={cn(
        "flip-number stat-value inline-flex items-end font-bold leading-none",
        tone === "primary" ? "flip-number-primary" : "flip-number-muted",
        className
      )}
      aria-label={safeValue}
    >
      {digits.map((digit, index) => (
        <RollingDigit
          key={`digit-${digits.length - index - 1}`}
          digit={Number(digit)}
          height={height}
          width={width}
          color={color}
          duration={duration}
        />
      ))}
    </span>
  );
}

type RollingDigitProps = {
  digit: number;
  height: number;
  width: number;
  color: string;
  duration: number;
};

type RollState = {
  direction: 1 | -1;
  sequence: number[];
  distance: number;
  key: number;
};

function RollingDigit({ digit, height, width, color, duration }: RollingDigitProps) {
  const previousDigit = useRef(digit);
  const animationKey = useRef(0);
  const [roll, setRoll] = useState<RollState | null>(null);

  useEffect(() => {
    const from = previousDigit.current;
    if (from === digit) {
      setRoll(null);
      return;
    }

    const forwardDistance = (digit - from + 10) % 10;
    const backwardDistance = (from - digit + 10) % 10;
    const direction: 1 | -1 = forwardDistance <= backwardDistance ? 1 : -1;
    const distance = direction === 1 ? forwardDistance : backwardDistance;
    const sequence = buildDigitSequence(from, digit, direction, distance);

    animationKey.current += 1;
    previousDigit.current = digit;
    setRoll({
      direction,
      sequence,
      distance,
      key: animationKey.current,
    });
  }, [digit]);

  const style = {
    "--rolling-digit-height": `${height}px`,
    "--rolling-digit-width": `${width}px`,
    "--rolling-digit-color": color,
    "--rolling-digit-duration": `${duration}ms`,
    "--rolling-digit-distance": `${(roll?.distance ?? 0) * height}px`,
  } as CSSProperties;

  return (
    <span className="rolling-digit-window flip-number-digit" style={style} aria-hidden="true">
      {roll ? (
        <span
          key={roll.key}
          className={cn(
            "rolling-digit-stack",
            roll.direction === 1 ? "is-forward" : "is-backward"
          )}
        >
          {roll.sequence.map((item, index) => (
            <span className="rolling-digit-cell" key={`${roll.key}-${item}-${index}`}>
              {item}
            </span>
          ))}
        </span>
      ) : (
        <span className="rolling-digit-cell">{digit}</span>
      )}
    </span>
  );
}

function buildDigitSequence(
  from: number,
  to: number,
  direction: 1 | -1,
  distance: number
) {
  if (direction === 1) {
    return Array.from({ length: distance + 1 }, (_, index) => (from + index) % 10);
  }

  return Array.from({ length: distance + 1 }, (_, index) => (to + index) % 10);
}
