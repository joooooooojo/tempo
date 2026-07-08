import * as React from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const weekdays = ["一", "二", "三", "四", "五", "六", "日"];

export interface CalendarProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "onSelect"> {
  month: Date;
  selected?: Date;
  isDateDisabled?: (date: Date) => boolean;
  onMonthChange: (month: Date) => void;
  onSelect: (date: Date) => void;
}

export function Calendar({
  month,
  selected,
  isDateDisabled,
  onMonthChange,
  onSelect,
  className,
  ...props
}: CalendarProps) {
  const days = React.useMemo(() => buildMonthDays(month), [month]);

  return (
    <div className={cn("rounded-lg p-3", className)} {...props}>
      <div className="mb-3 flex items-center justify-between">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          aria-label="上个月"
          onClick={() => onMonthChange(addMonths(month, -1))}
        >
          <ChevronLeft className="h-4 w-4" />
        </Button>
        <p className="text-[13px] font-semibold">
          {new Intl.DateTimeFormat("zh-CN", {
            year: "numeric",
            month: "long",
          }).format(month)}
        </p>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          aria-label="下个月"
          onClick={() => onMonthChange(addMonths(month, 1))}
        >
          <ChevronRight className="h-4 w-4" />
        </Button>
      </div>

      <div className="grid grid-cols-7 gap-1 text-center">
        {weekdays.map((weekday) => (
          <span
            key={weekday}
            className="flex h-7 items-center justify-center text-[11px] font-medium text-muted-foreground"
          >
            {weekday}
          </span>
        ))}
        {days.map((date) => {
          const inCurrentMonth = date.getMonth() === month.getMonth();
          const selectedDay = Boolean(selected && isSameDay(date, selected));
          const today = isSameDay(date, new Date());
          const disabled = isDateDisabled?.(date) ?? false;

          return (
            <button
              key={date.toISOString()}
              type="button"
              disabled={disabled}
              className={cn(
                "flex h-8 w-8 items-center justify-center rounded-md text-[13px] font-medium transition-colors",
                inCurrentMonth
                  ? "text-foreground hover:bg-foreground/7"
                  : "text-muted-foreground/40 hover:bg-foreground/5",
                today && !selectedDay && "bg-primary/10 text-primary",
                selectedDay && "bg-primary text-primary-foreground shadow-sm shadow-primary/20",
                disabled && "cursor-default opacity-35 hover:bg-transparent"
              )}
              onClick={() => onSelect(date)}
            >
              {date.getDate()}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function buildMonthDays(month: Date) {
  const firstOfMonth = new Date(month.getFullYear(), month.getMonth(), 1);
  const firstWeekdayOffset = (firstOfMonth.getDay() + 6) % 7;
  const start = new Date(firstOfMonth);
  start.setDate(firstOfMonth.getDate() - firstWeekdayOffset);

  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(start);
    date.setDate(start.getDate() + index);
    return date;
  });
}

function addMonths(date: Date, months: number) {
  return new Date(date.getFullYear(), date.getMonth() + months, 1);
}

function isSameDay(a: Date, b: Date) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}
