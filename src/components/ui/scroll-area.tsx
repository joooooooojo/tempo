import * as React from "react"
import { ScrollArea as ScrollAreaPrimitive } from "@base-ui/react/scroll-area"

import { cn } from "@/lib/utils"

type ScrollAreaProps = ScrollAreaPrimitive.Root.Props & {
  scrollbarClassName?: string
  scrollbars?: "vertical" | "horizontal" | "both" | "none"
  thumbClassName?: string
  verticalScrollbarInsetTop?: React.CSSProperties["top"]
  viewportClassName?: string
  viewportRef?: React.Ref<HTMLDivElement>
}

function ScrollArea({
  className,
  children,
  scrollbarClassName,
  scrollbars = "vertical",
  thumbClassName,
  verticalScrollbarInsetTop,
  viewportClassName,
  viewportRef,
  ...props
}: ScrollAreaProps) {
  return (
    <ScrollAreaPrimitive.Root
      data-slot="scroll-area"
      className={cn("relative", className)}
      {...props}
    >
      <ScrollAreaPrimitive.Viewport
        data-slot="scroll-area-viewport"
        ref={viewportRef}
        className={cn(
          "size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1",
          viewportClassName
        )}
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      {(scrollbars === "vertical" || scrollbars === "both") && (
        <ScrollBar
          className={scrollbarClassName}
          thumbClassName={thumbClassName}
          style={
            verticalScrollbarInsetTop === undefined
              ? undefined
              : { top: verticalScrollbarInsetTop }
          }
        />
      )}
      {(scrollbars === "horizontal" || scrollbars === "both") && (
        <ScrollBar
          orientation="horizontal"
          className={scrollbarClassName}
          thumbClassName={thumbClassName}
        />
      )}
      <ScrollAreaPrimitive.Corner />
    </ScrollAreaPrimitive.Root>
  )
}

function ScrollBar({
  className,
  orientation = "vertical",
  thumbClassName,
  ...props
}: ScrollAreaPrimitive.Scrollbar.Props & { thumbClassName?: string }) {
  return (
    <ScrollAreaPrimitive.Scrollbar
      data-slot="scroll-area-scrollbar"
      data-orientation={orientation}
      orientation={orientation}
      className={cn(
        "flex touch-none p-px transition-opacity select-none data-horizontal:h-2.5 data-horizontal:flex-col data-vertical:w-2.5",
        "pointer-events-none opacity-0 duration-150",
        "data-hovering:pointer-events-auto data-hovering:opacity-100",
        "data-scrolling:pointer-events-auto data-scrolling:opacity-100 data-scrolling:duration-0",
        className
      )}
      {...props}
    >
      <ScrollAreaPrimitive.Thumb
        data-slot="scroll-area-thumb"
        className={cn("relative flex-1 rounded-full bg-border", thumbClassName)}
      />
    </ScrollAreaPrimitive.Scrollbar>
  )
}

export { ScrollArea, ScrollBar }
