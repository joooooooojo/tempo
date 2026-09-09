import type { ToasterProps } from "sonner";

export const appToastOptions = {
  className: "glass rounded-lg",
  classNames: {
    actionButton:
      "!h-8 !rounded-md !border !border-emerald-200 !bg-emerald-50 !px-3 !text-[13px] !font-medium !text-emerald-700 !shadow-none transition-colors hover:!bg-emerald-100 focus-visible:!ring-2 focus-visible:!ring-emerald-300 dark:!border-emerald-400/25 dark:!bg-emerald-500/15 dark:!text-emerald-100 dark:hover:!bg-emerald-500/20",
  },
} satisfies ToasterProps["toastOptions"];
