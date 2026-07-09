import * as React from "react";
import { Branch as DismissableLayerBranch } from "@radix-ui/react-dismissable-layer";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { cn } from "@/lib/utils";

const Popover = PopoverPrimitive.Root;
const PopoverTrigger = PopoverPrimitive.Trigger;
const PopoverAnchor = PopoverPrimitive.Anchor;

type PopoverContentProps = React.ComponentPropsWithoutRef<typeof PopoverPrimitive.Content> & {
  portalled?: boolean;
  /** @deprecated Prefer `overlayLayer` to portal into `document.body` with a raised z-index. */
  container?: HTMLElement | null;
  /** Portal to `document.body` above dialogs (Floating UI / Popper positioning). */
  overlayLayer?: boolean;
};

const PopoverContent = React.forwardRef<
  React.ElementRef<typeof PopoverPrimitive.Content>,
  PopoverContentProps
>(
  (
    {
      className,
      align = "start",
      sideOffset = 8,
      portalled = true,
      container,
      overlayLayer = false,
      ...props
    },
    ref
  ) => {
    const content = (
      <PopoverPrimitive.Content
        ref={ref}
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "rounded-lg border border-border/80 bg-popover/95 text-popover-foreground shadow-xl shadow-emerald-950/10 outline-none backdrop-blur-xl data-[state=open]:animate-in data-[state=closed]:animate-out",
          overlayLayer ? "pointer-events-auto z-[120]" : "z-50",
          className
        )}
        {...props}
      />
    );

    if (!portalled) return content;

    const portalledContent = overlayLayer ? (
      <DismissableLayerBranch className="contents">{content}</DismissableLayerBranch>
    ) : (
      content
    );

    return (
      <PopoverPrimitive.Portal container={overlayLayer ? undefined : container}>
        {portalledContent}
      </PopoverPrimitive.Portal>
    );
  }
);
PopoverContent.displayName = PopoverPrimitive.Content.displayName;

export { Popover, PopoverAnchor, PopoverContent, PopoverTrigger };
