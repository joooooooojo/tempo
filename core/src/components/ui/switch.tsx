import { Switch as SwitchPrimitive } from "@base-ui/react/switch"

import { cn } from "@/lib/utils"

function Switch({
  className,
  size = "default",
  ...props
}: SwitchPrimitive.Root.Props & {
  size?: "sm" | "default"
}) {
  const isSm = size === "sm"

  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      data-size={size}
      className={cn(
        "peer inline-flex shrink-0 cursor-pointer items-center rounded-lg bg-foreground/10 transition-colors duration-200 ease-out outline-none focus-visible:ring-2 focus-visible:ring-ring data-checked:bg-primary data-disabled:cursor-not-allowed data-disabled:opacity-50",
        isSm ? "h-[22px] w-[38px]" : "h-[26px] w-[46px]",
        className
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(
          // Tailwind v4 `translate-x-*` sets CSS `translate`, not `transform` —
          // `transition-transform` alone won't animate the thumb.
          "pointer-events-none block rounded-md bg-white shadow-md transition-[transform,translate] duration-200 ease-out will-change-transform data-unchecked:translate-x-[2px]",
          isSm
            ? "h-[18px] w-[18px] data-checked:translate-x-[18px]"
            : "h-[22px] w-[22px] data-checked:translate-x-[22px]"
        )}
      />
    </SwitchPrimitive.Root>
  )
}

export { Switch }
