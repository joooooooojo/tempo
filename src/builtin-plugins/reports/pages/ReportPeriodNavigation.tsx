import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";

interface ReportPeriodNavigationProps {
  label: string;
  dateTime: string;
  groupLabel: string;
  previousLabel: string;
  nextLabel: string;
  nextDisabledLabel: string;
  nextDisabled: boolean;
  onPrevious: () => void;
  onNext: () => void;
}

export function ReportPeriodNavigation({
  label,
  dateTime,
  groupLabel,
  previousLabel,
  nextLabel,
  nextDisabledLabel,
  nextDisabled,
  onPrevious,
  onNext,
}: ReportPeriodNavigationProps) {
  return (
    <div
      className="flex items-center gap-1 justify-self-center rounded-lg border border-border/70 bg-muted/45 p-0.5 max-[640px]:col-span-2 max-[640px]:col-start-1 max-[640px]:row-start-2"
      role="group"
      aria-label={groupLabel}
    >
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={`查看${previousLabel}`}
        title={previousLabel}
        onClick={onPrevious}
      >
        <ChevronLeft />
      </Button>
      <time
        dateTime={dateTime}
        className="w-[132px] text-center text-[12px] font-semibold tabular-nums text-foreground"
        aria-live="polite"
      >
        {label}
      </time>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={`查看${nextLabel}`}
        title={nextDisabled ? nextDisabledLabel : nextLabel}
        disabled={nextDisabled}
        onClick={onNext}
      >
        <ChevronRight />
      </Button>
    </div>
  );
}

