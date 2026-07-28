import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";

type FollowTooltipProps = {
  content: ReactNode;
  children: ReactNode;
  className?: string;
  disabled?: boolean;
};

const OFFSET = 14;

/**
 * Lightweight cursor-following tooltip (not the native `title` attribute).
 * Renders in a portal so it is not clipped by overflow parents.
 */
export function FollowTooltip({
  content,
  children,
  className,
  disabled = false,
}: FollowTooltipProps) {
  const tipId = useId();
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const frameRef = useRef(0);

  const updatePosition = useCallback((clientX: number, clientY: number) => {
    window.cancelAnimationFrame(frameRef.current);
    frameRef.current = window.requestAnimationFrame(() => {
      const maxX = window.innerWidth - 16;
      const maxY = window.innerHeight - 16;
      setPos({
        x: Math.min(clientX + OFFSET, maxX),
        y: Math.min(clientY + OFFSET, maxY),
      });
    });
  }, []);

  useEffect(() => {
    return () => window.cancelAnimationFrame(frameRef.current);
  }, []);

  if (disabled || content == null || content === "") {
    return <>{children}</>;
  }

  return (
    <span
      className={cn("follow-tooltip-trigger", className)}
      aria-describedby={open ? tipId : undefined}
      onMouseEnter={(event) => {
        setOpen(true);
        updatePosition(event.clientX, event.clientY);
      }}
      onMouseMove={(event) => updatePosition(event.clientX, event.clientY)}
      onMouseLeave={() => setOpen(false)}
    >
      {children}
      {open
        ? createPortal(
            <span
              id={tipId}
              role="tooltip"
              className="follow-tooltip"
              style={{ left: pos.x, top: pos.y }}
            >
              {content}
            </span>,
            document.body
          )
        : null}
    </span>
  );
}
