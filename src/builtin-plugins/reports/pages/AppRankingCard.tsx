import {
  memo,
  useLayoutEffect,
  useRef,
} from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AppIcon } from "@/components/AppIcon";
import { formatDuration, formatDurationShort } from "@/lib/utils";
import type { AppUsage } from "@/types";
import { EmptyState } from "@/builtin-plugins/reports/pages/reportUtils";

interface AppRankingCardProps {
  apps: AppUsage[];
  periodKey: string;
}

export const AppRankingCard = memo(function AppRankingCard({
  apps,
  periodKey,
}: AppRankingCardProps) {
  const contentRef = useRef<HTMLDivElement>(null);
  const previousPositionsRef = useRef<Map<string, DOMRect>>(new Map());
  const previousPeriodRef = useRef<string | null>(null);

  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content) return;

    const rows = Array.from(content.querySelectorAll<HTMLElement>("[data-app-key]"));
    const nextPositions = new Map(
      rows.map((row) => [row.dataset.appKey ?? "", row.getBoundingClientRect()])
    );
    const shouldAnimate = previousPeriodRef.current !== null
      && previousPeriodRef.current !== periodKey
      && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const animations: Animation[] = [];

    if (shouldAnimate) {
      rows.forEach((row, index) => {
        const appKey = row.dataset.appKey ?? "";
        const previousRect = previousPositionsRef.current.get(appKey);
        const nextRect = nextPositions.get(appKey);
        const delay = Math.min(index * 22, 110);

        if (previousRect && nextRect) {
          const deltaX = previousRect.left - nextRect.left;
          const deltaY = previousRect.top - nextRect.top;

          if (Math.abs(deltaX) > 0.5 || Math.abs(deltaY) > 0.5) {
            animations.push(row.animate(
              [
                { transform: `translate(${deltaX}px, ${deltaY}px)` },
                { transform: "translate(0, 0)" },
              ],
              {
                duration: 420,
                delay,
                easing: "cubic-bezier(0.22, 1, 0.36, 1)",
              }
            ));
          }
          return;
        }

        animations.push(row.animate(
          [
            { opacity: 0, transform: "translateY(8px) scale(0.985)" },
            { opacity: 1, transform: "translateY(0) scale(1)" },
          ],
          {
            duration: 300,
            delay,
            easing: "cubic-bezier(0.22, 1, 0.36, 1)",
          }
        ));
      });
    }

    previousPositionsRef.current = nextPositions;
    previousPeriodRef.current = periodKey;

    return () => animations.forEach((animation) => animation.cancel());
  }, [apps, periodKey]);

  return (
    <Card>
      <CardHeader className="pb-0"><CardTitle>应用排名</CardTitle></CardHeader>
      <CardContent ref={contentRef} className="p-0 pt-2">
        {apps.length === 0 ? <div className="px-4 pb-4"><EmptyState /></div> : (
          apps.map((app, index) => (
            <div key={app.app_name} data-app-key={app.app_name} className="list-row">
              <span className="flex min-w-0 items-center gap-3 text-[13px]">
                <span className="w-5 text-[11px] font-bold text-muted-foreground">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <AppIcon
                  name={app.app_name}
                  iconDataUrl={app.icon_data_url}
                  size="sm"
                  fallbackClassName="bg-gradient-to-br from-slate-400 to-slate-600 text-white"
                />
                <span className="truncate">{app.app_name}</span>
              </span>
              <span className="stat-value shrink-0 text-[13px] font-semibold text-primary">
                {formatDurationShort(app.seconds)}
              </span>
            </div>
          ))
        )}
      </CardContent>
    </Card>
  );
});

interface DurationTooltipProps {
  active?: boolean;
  label?: string | number;
  payload?: Array<{
    value?: number | string;
    payload?: {
      label?: string;
    };
  }>;
}

export function DurationTooltip({ active, label, payload }: DurationTooltipProps) {
  if (!active || !payload?.length) return null;

  const seconds = Number(payload[0].value ?? 0);
  const displayLabel = payload[0].payload?.label ?? label;

  return (
    <div className="rounded-lg border border-border/80 bg-popover/92 px-3 py-2 text-[12px] shadow-lg shadow-emerald-950/5 backdrop-blur">
      <p className="font-semibold text-foreground">{displayLabel}</p>
      <p className="mt-1 text-muted-foreground">
        使用时长 <span className="font-semibold text-primary">{formatDuration(seconds)}</span>
      </p>
    </div>
  );
}

