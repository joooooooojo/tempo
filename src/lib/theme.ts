import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Settings } from "@/types";

export const THEME_CHANGED_EVENT = "settings:theme-changed";

export function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;
  if (theme === "dark") {
    root.classList.add("dark");
  } else if (theme === "light") {
    root.classList.remove("dark");
  } else {
    root.classList.toggle(
      "dark",
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  }
  void syncNativeWindowTheme(theme);
}

/** Keep the Tauri/native window appearance aligned with the CSS theme. */
async function syncNativeWindowTheme(theme: Settings["theme"]) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    const native = theme === "system" ? null : theme;
    await getCurrentWindow().setTheme(native);
    // Keep the native main-panel fill aligned after appearance changes.
    const label = getCurrentWindow().label;
    if (label === "main-panel") {
      await invoke("sync_main_panel_appearance");
    }
  } catch {
    // Some auxiliary windows may not support setTheme; ignore.
  }
}

export async function emitThemeChange(theme: Settings["theme"]) {
  applyTheme(theme);
  await emit(THEME_CHANGED_EVENT, { theme });
}

export function subscribeThemeChanges(
  onTheme: (theme: Settings["theme"]) => void
): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;

  void listen<{ theme: Settings["theme"] }>(THEME_CHANGED_EVENT, (event) => {
    if (!disposed) onTheme(event.payload.theme);
  }).then((fn) => {
    if (disposed) {
      fn();
      return;
    }
    unlisten = fn;
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}

export function watchSystemTheme(
  getTheme: () => Settings["theme"],
  onSystemChange: () => void
): () => void {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => {
    if (getTheme() === "system") onSystemChange();
  };
  media.addEventListener("change", handler);
  return () => media.removeEventListener("change", handler);
}
