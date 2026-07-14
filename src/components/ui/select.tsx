"use client"

import * as React from "react"
import { Select as SelectPrimitive } from "@base-ui/react/select"
import { ChevronDownIcon, CheckIcon, ChevronUpIcon, SearchIcon } from "lucide-react"

import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"

type SelectRootProps<Value = any, Multiple extends boolean | undefined = false> =
  SelectPrimitive.Root.Props<Value, Multiple>

const SelectOpenContext = React.createContext(false)

function Select<Value = any, Multiple extends boolean | undefined = false>({
  onOpenChange,
  ...props
}: SelectRootProps<Value, Multiple>) {
  const [open, setOpen] = React.useState(false)

  return (
    <SelectPrimitive.Root
      {...props}
      onOpenChange={(next, eventDetails) => {
        setOpen(next)
        onOpenChange?.(next, eventDetails)
      }}
    >
      <SelectOpenContext.Provider value={open}>{props.children}</SelectOpenContext.Provider>
    </SelectPrimitive.Root>
  )
}

function SelectGroup({ className, ...props }: SelectPrimitive.Group.Props) {
  return (
    <SelectPrimitive.Group
      data-slot="select-group"
      className={cn("scroll-my-1", className)}
      {...props}
    />
  )
}

function SelectValue({ className, ...props }: SelectPrimitive.Value.Props) {
  return (
    <SelectPrimitive.Value
      data-slot="select-value"
      className={cn("flex min-w-0 flex-1 overflow-hidden text-left", className)}
      {...props}
    />
  )
}

function SelectTrigger({
  className,
  size = "default",
  children,
  ...props
}: SelectPrimitive.Trigger.Props & {
  size?: "sm" | "default"
}) {
  return (
    <SelectPrimitive.Trigger
      data-slot="select-trigger"
      data-size={size}
      className={cn(
        "flex w-fit items-center justify-between gap-1.5 rounded-lg border border-input bg-background pr-2 pl-3 text-[13px] whitespace-nowrap shadow-xs transition-colors outline-none select-none hover:bg-accent/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-placeholder:text-muted-foreground data-[size=default]:h-9 data-[size=sm]:h-8 data-[size=sm]:rounded-[min(var(--radius-md),10px)] *:data-[slot=select-value]:truncate *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-1.5 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon
        render={
          <ChevronDownIcon className="pointer-events-none size-4 text-muted-foreground" />
        }
      />
    </SelectPrimitive.Trigger>
  )
}

function getNodeText(node: React.ReactNode): string {
  if (node == null || typeof node === "boolean") return ""
  if (typeof node === "string" || typeof node === "number") return String(node)
  if (Array.isArray(node)) return node.map(getNodeText).join("")
  if (React.isValidElement<{ children?: React.ReactNode }>(node)) {
    return getNodeText(node.props.children)
  }
  return ""
}

function matchesQuery(query: string, label: string, value: unknown) {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return (
    label.toLowerCase().includes(q) || String(value ?? "").toLowerCase().includes(q)
  )
}

function filterSelectChildren(
  children: React.ReactNode,
  query: string
): { nodes: React.ReactNode; count: number } {
  let count = 0

  const nodes = React.Children.map(children, (child) => {
    if (!React.isValidElement(child)) return child

    const slot = (child.props as { "data-slot"?: string })["data-slot"]
    const childProps = child.props as {
      children?: React.ReactNode
      value?: unknown
    }

    if (slot === "select-item" || child.type === SelectItem) {
      const label = getNodeText(childProps.children)
      if (!matchesQuery(query, label, childProps.value)) return null
      count += 1
      return child
    }

    if (slot === "select-group" || child.type === SelectGroup) {
      const filtered = filterSelectChildren(childProps.children, query)
      count += filtered.count
      if (filtered.count === 0) return null
      return React.cloneElement(child, undefined, filtered.nodes)
    }

    return child
  })

  return { nodes, count }
}

function SelectContent({
  className,
  children,
  side = "bottom",
  sideOffset = 4,
  align = "center",
  alignOffset = 0,
  alignItemWithTrigger = true,
  overlayLayer = false,
  searchable = false,
  searchPlaceholder = "搜索...",
  ...props
}: SelectPrimitive.Popup.Props &
  Pick<
    SelectPrimitive.Positioner.Props,
    "align" | "alignOffset" | "side" | "sideOffset" | "alignItemWithTrigger"
  > & {
    searchable?: boolean
    searchPlaceholder?: string
    /** Raise above nested dialogs / modals */
    overlayLayer?: boolean
  }) {
  const open = React.useContext(SelectOpenContext)
  const [query, setQuery] = React.useState("")
  const inputRef = React.useRef<HTMLInputElement>(null)

  React.useEffect(() => {
    if (!open) {
      setQuery("")
      return
    }
    if (!searchable) return
    const id = window.requestAnimationFrame(() => inputRef.current?.focus())
    return () => window.cancelAnimationFrame(id)
  }, [open, searchable])

  const { nodes: filteredChildren, count: matchCount } = React.useMemo(() => {
    if (!searchable || !query.trim()) {
      return { nodes: children, count: -1 }
    }
    return filterSelectChildren(children, query)
  }, [children, query, searchable])

  return (
    <SelectPrimitive.Portal>
        <SelectPrimitive.Positioner
          side={side}
          sideOffset={sideOffset}
          align={align}
          alignOffset={alignOffset}
          alignItemWithTrigger={searchable ? false : alignItemWithTrigger}
          className={cn("isolate", overlayLayer ? "z-[80]" : "z-50")}
        >
          <SelectPrimitive.Popup
            data-slot="select-content"
            data-align-trigger={searchable ? false : alignItemWithTrigger}
            className={cn(
              "relative isolate flex w-[calc(var(--anchor-width)+0.75rem)] max-w-[calc(var(--anchor-width)+0.75rem)] min-w-0 origin-(--transform-origin) flex-col overflow-hidden rounded-xl bg-popover text-[13px] text-popover-foreground shadow-lg ring-1 ring-border duration-100 data-[align-trigger=true]:animate-none data-[side=bottom]:slide-in-from-top-2 data-[side=inline-end]:slide-in-from-left-2 data-[side=inline-start]:slide-in-from-right-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
              searchable
                ? "max-h-[min(var(--available-height),20rem)] p-0"
                : "max-h-[min(var(--available-height),16rem)] overflow-x-hidden overflow-y-auto overscroll-contain p-1.5",
              overlayLayer ? "z-[80]" : "z-50",
              className
            )}
            {...props}
          >
            {searchable && (
              <div className="sticky top-0 z-10 shrink-0 border-b border-border/60 bg-popover p-1.5">
                <div className="relative">
                  <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    ref={inputRef}
                    value={query}
                    placeholder={searchPlaceholder}
                    className="h-8 border-0 bg-transparent pr-2.5 pl-8 shadow-none focus-visible:ring-0 dark:bg-transparent"
                    onChange={(event) => setQuery(event.target.value)}
                    onKeyDown={(event) => {
                      // Keep typing in the search box; let arrows/enter reach the list.
                      if (
                        event.key === "ArrowDown" ||
                        event.key === "ArrowUp" ||
                        event.key === "Enter" ||
                        event.key === "Escape" ||
                        event.key === "Home" ||
                        event.key === "End"
                      ) {
                        return
                      }
                      event.stopPropagation()
                    }}
                    onClick={(event) => event.stopPropagation()}
                  />
                </div>
              </div>
            )}
            <div
              className={cn(
                searchable &&
                  "min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain p-1.5"
              )}
            >
              <SelectScrollUpButton />
              <SelectPrimitive.List>
                {filteredChildren}
                {searchable && matchCount === 0 && (
                  <div className="px-3 py-6 text-center text-[12px] text-muted-foreground">
                    无匹配项
                  </div>
                )}
              </SelectPrimitive.List>
              <SelectScrollDownButton />
            </div>
          </SelectPrimitive.Popup>
        </SelectPrimitive.Positioner>
      </SelectPrimitive.Portal>
  )
}

function SelectLabel({
  className,
  ...props
}: SelectPrimitive.GroupLabel.Props) {
  return (
    <SelectPrimitive.GroupLabel
      data-slot="select-label"
      className={cn("px-1.5 py-1 text-xs text-muted-foreground", className)}
      {...props}
    />
  )
}

function SelectItem({
  className,
  children,
  ...props
}: SelectPrimitive.Item.Props) {
  return (
    <SelectPrimitive.Item
      data-slot="select-item"
      className={cn(
        "relative flex h-9 w-full cursor-pointer items-center gap-2 rounded-lg pr-9 pl-3 text-[13px] outline-hidden select-none focus:bg-accent focus:text-accent-foreground not-data-[variant=destructive]:focus:**:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2",
        className
      )}
      {...props}
    >
      <SelectPrimitive.ItemText className="min-w-0 max-w-full flex-1 truncate whitespace-nowrap">
        {children}
      </SelectPrimitive.ItemText>
      <SelectPrimitive.ItemIndicator
        render={
          <span className="pointer-events-none absolute right-3 flex size-4 items-center justify-center" />
        }
      >
        <CheckIcon className="pointer-events-none" />
      </SelectPrimitive.ItemIndicator>
    </SelectPrimitive.Item>
  )
}

function SelectSeparator({
  className,
  ...props
}: SelectPrimitive.Separator.Props) {
  return (
    <SelectPrimitive.Separator
      data-slot="select-separator"
      className={cn("pointer-events-none -mx-1 my-1 h-px bg-border", className)}
      {...props}
    />
  )
}

function SelectScrollUpButton({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollUpArrow>) {
  return (
    <SelectPrimitive.ScrollUpArrow
      data-slot="select-scroll-up-button"
      className={cn(
        "top-0 z-10 flex w-full cursor-default items-center justify-center bg-popover py-1 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      <ChevronUpIcon
      />
    </SelectPrimitive.ScrollUpArrow>
  )
}

function SelectScrollDownButton({
  className,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollDownArrow>) {
  return (
    <SelectPrimitive.ScrollDownArrow
      data-slot="select-scroll-down-button"
      className={cn(
        "bottom-0 z-10 flex w-full cursor-default items-center justify-center bg-popover py-1 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    >
      <ChevronDownIcon
      />
    </SelectPrimitive.ScrollDownArrow>
  )
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
}
