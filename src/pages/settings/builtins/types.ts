import type { ComponentType } from "react";

/** Props for built-in plugin custom config panels (green channel). */
export type BuiltinConfigPanelProps = {
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
};

export type BuiltinConfigPanel = ComponentType<BuiltinConfigPanelProps>;
