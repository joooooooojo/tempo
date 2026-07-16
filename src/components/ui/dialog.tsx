import * as React from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { X } from "lucide-react"

function Dialog({ ...props }: DialogPrimitive.Root.Props) {
  return <DialogPrimitive.Root data-slot="dialog" {...props} />
}

function DialogTrigger({
  asChild,
  children,
  ...props
}: DialogPrimitive.Trigger.Props & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <DialogPrimitive.Trigger data-slot="dialog-trigger" render={children} {...props} />
  }

  return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props}>{children}</DialogPrimitive.Trigger>
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
  return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />
}

function DialogClose({
  asChild,
  children,
  ...props
}: DialogPrimitive.Close.Props & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <DialogPrimitive.Close data-slot="dialog-close" render={children} {...props} />
  }

  return <DialogPrimitive.Close data-slot="dialog-close" {...props}>{children}</DialogPrimitive.Close>
}

function DialogOverlay({
  className,
  ...props
}: DialogPrimitive.Backdrop.Props) {
  return (
    <DialogPrimitive.Backdrop
      data-slot="dialog-overlay"
      className={cn(
        "fixed inset-0 isolate z-50 bg-black/10 duration-100 supports-backdrop-filter:backdrop-blur-xs data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
        className
      )}
      {...props}
    />
  )
}

/** Outer popup shell. Compose with DialogHeader / DialogContent / DialogFooter. */
function DialogPanel({
  className,
  children,
  showOverlay = true,
  onOpenAutoFocus: _onOpenAutoFocus,
  ...props
}: DialogPrimitive.Popup.Props & {
  /** First-layer dialogs keep the dimmed mask; nested layers usually omit it. */
  showOverlay?: boolean
  onOpenAutoFocus?: (event: { preventDefault: () => void }) => void
}) {
  return (
    <DialogPortal>
      {showOverlay ? <DialogOverlay /> : null}
      <DialogPrimitive.Popup
        data-slot="dialog-panel"
        data-nested={showOverlay ? undefined : "true"}
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-0 overflow-hidden rounded-xl bg-popover p-0 text-sm text-popover-foreground ring-1 ring-foreground/10 duration-100 outline-none sm:max-w-sm data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          !showOverlay && "dialog-nested z-[70]",
          className
        )}
        {...props}
      >
        {children}
      </DialogPrimitive.Popup>
    </DialogPortal>
  )
}

function DialogHeader({
  className,
  children,
  showCloseButton = true,
  ...props
}: React.ComponentProps<"div"> & {
  showCloseButton?: boolean
}) {
  return (
    <div
      data-slot="dialog-header"
      className={cn(
        "flex shrink-0 border-b border-border/60 px-6 py-4 text-left",
        showCloseButton ? "items-start gap-3" : "flex-col gap-1.5",
        className
      )}
      {...props}
    >
      {showCloseButton ? (
        <>
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">{children}</div>
          <DialogPrimitive.Close
            data-slot="dialog-close"
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                className="shrink-0"
                aria-label="关闭"
              />
            }
          >
            <X />
            <span className="sr-only">关闭</span>
          </DialogPrimitive.Close>
        </>
      ) : (
        children
      )}
    </div>
  )
}

/** Dialog body — between header and footer. */
function DialogContent({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="dialog-content"
      className={cn("min-h-0 flex-1 overflow-y-auto px-6 py-5", className)}
      {...props}
    />
  )
}

function DialogFooter({
  className,
  showCloseButton = false,
  children,
  ...props
}: React.ComponentProps<"div"> & {
  showCloseButton?: boolean
}) {
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        "flex shrink-0 flex-col-reverse gap-2 border-t border-border/60 bg-muted/50 px-6 py-4 sm:flex-row sm:items-center sm:justify-end",
        className
      )}
      {...props}
    >
      {children}
      {showCloseButton && (
        <DialogPrimitive.Close render={<Button variant="outline" />}>
          Close
        </DialogPrimitive.Close>
      )}
    </div>
  )
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      data-slot="dialog-title"
      className={cn(
        "font-heading flex min-h-7 items-center text-[15px] leading-none font-semibold tracking-tight",
        className
      )}
      {...props}
    />
  )
}

function DialogDescription({
  asChild,
  children,
  className,
  ...props
}: DialogPrimitive.Description.Props & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    const child = children as React.ReactElement<{ className?: string }>

    return (
      <DialogPrimitive.Description
        data-slot="dialog-description"
        render={React.cloneElement(child, {
          className: cn(
            "text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
            child.props.className,
            className
          ),
        })}
        {...props}
      />
    )
  }

  return (
    <DialogPrimitive.Description
      data-slot="dialog-description"
      className={cn(
        "text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
        className
      )}
      {...props}
    >
      {children}
    </DialogPrimitive.Description>
  )
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPanel,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
}
