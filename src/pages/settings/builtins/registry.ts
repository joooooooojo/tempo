import { ClipboardBuiltinConfig } from "@/pages/settings/builtins/ClipboardBuiltinConfig";
import type { BuiltinConfigPanel } from "@/pages/settings/builtins/types";

/**
 * Green channel: built-in apps may register free-form config panels that are not
 * constrained by third-party `contributes.settings` field types.
 */
const BUILTIN_CONFIG_PANELS: Record<string, BuiltinConfigPanel> = {
  clipboard: ClipboardBuiltinConfig,
};

export function getBuiltinConfigPanel(appId: string): BuiltinConfigPanel | null {
  return BUILTIN_CONFIG_PANELS[appId] ?? null;
}

export function hasBuiltinConfigPanel(appId: string): boolean {
  return appId in BUILTIN_CONFIG_PANELS;
}
