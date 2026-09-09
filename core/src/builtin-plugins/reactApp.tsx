import type { ComponentType } from "react";
import type { TempoApp, TempoAppProps } from "@/apps/types";

export function wrapPage(Page: ComponentType): ComponentType<TempoAppProps> {
  return function BuiltinAppPage(_props: TempoAppProps) {
    return <Page />;
  };
}

export function reactApp(
  partial: Omit<TempoApp, "source" | "ui"> & {
    component: ComponentType<TempoAppProps>;
  },
): TempoApp {
  const { component, ...rest } = partial;
  return {
    ...rest,
    source: "builtin",
    ui: { type: "react", component },
  };
}
