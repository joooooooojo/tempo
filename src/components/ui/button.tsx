import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap text-[13px] font-semibold transition-[background,box-shadow,color,transform,filter] duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/70 disabled:pointer-events-none disabled:opacity-50 active:scale-[0.985]",
  {
    variants: {
      variant: {
        default: "rounded-lg bg-primary text-primary-foreground shadow-sm shadow-primary/20 hover:brightness-105",
        destructive: "rounded-lg bg-rose-500/12 text-rose-700 shadow-none hover:bg-rose-500/18 dark:text-rose-300 dark:hover:bg-rose-500/20",
        outline: "rounded-lg glass-subtle text-foreground hover:bg-white/45 dark:hover:bg-white/8",
        secondary: "rounded-lg bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ghost: "rounded-lg hover:bg-foreground/5",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-4",
        sm: "h-8 rounded-lg px-3 text-xs",
        lg: "h-10 rounded-lg px-6",
        icon: "h-9 w-9 rounded-lg",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return <Comp className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props} />;
  }
);
Button.displayName = "Button";

export { Button, buttonVariants };
