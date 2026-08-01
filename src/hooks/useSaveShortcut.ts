import { useEffect, useRef } from "react";
import { isMacTarget } from "@/lib/utils";

/** Platform-native save chord for button titles / tooltips. */
export const SAVE_SHORTCUT_LABEL = isMacTarget ? "⌘S" : "Ctrl+S";

type UseSaveShortcutOptions = {
  /** When false, the chord is swallowed but `onSave` is not called. Default true. */
  enabled?: boolean;
  /** When false, no listener is attached. Default true. */
  active?: boolean;
};

/**
 * Call `onSave` on ⌘S / Ctrl+S. Alt/Shift variants are ignored so Save As stays free.
 * While active, the native WebView "Save page" dialog is always prevented.
 */
export function useSaveShortcut(
  onSave: () => void,
  options: UseSaveShortcutOptions = {},
) {
  const { enabled = true, active = true } = options;
  const onSaveRef = useRef(onSave);
  const enabledRef = useRef(enabled);
  onSaveRef.current = onSave;
  enabledRef.current = enabled;

  useEffect(() => {
    if (!active) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      if (event.key !== "s" && event.key !== "S") return;
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) {
        return;
      }

      event.preventDefault();
      if (!enabledRef.current) return;
      onSaveRef.current();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active]);
}
