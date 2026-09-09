import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Settings } from "@/types";

export const THEME_CHANGED_EVENT = "settings:theme-changed";

/** Pushed from Rust when the OS light/dark preference changes (see `platform::start_system_appearance_watcher`). */
export const SYSTEM_APPEARANCE_CHANGED_EVENT = "os:appearance-changed";

export function applyTheme(theme: Settings["theme"]) {
  void applyThemeAsync(theme);
}

/**
 * Keep CSS `.dark` and the native window appearance aligned with the setting.
 *
 * "system" is resolved to a concrete light/dark. On macOS, `setTheme(null)` does not
 * reliably drive WKWebView / shelf vibrancy for LSUIElement apps; explicit themes do.
 */
export async function applyThemeAsync(theme: Settings["theme"]) {
  const root = document.documentElement;
  // Boot script may set an inline background for the splash; clear so tokens win.
  if (root.style.background) {
    root.style.removeProperty("background");
  }

  const resolved = await resolveTheme(theme);
  root.classList.toggle("dark", resolved === "dark");
  root.style.colorScheme = resolved;
  await syncNativeWindowTheme(resolved);
}

export async function resolveTheme(
  theme: Settings["theme"]
): Promise<"light" | "dark"> {
  if (theme === "dark" || theme === "light") return theme;
  return (await resolveSystemIsDark()) ? "dark" : "light";
}

async function resolveSystemIsDark(): Promise<boolean> {
  if ("__TAURI_INTERNALS__" in window) {
    try {
      return await invoke<boolean>("system_prefers_dark");
    } catch {
      try {
        const native = await getCurrentWindow().theme();
        if (native === "dark") return true;
        if (native === "light") return false;
      } catch {
        // fall through to matchMedia
      }
    }
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Drive native chrome with an explicit light/dark (never null / system). */
async function syncNativeWindowTheme(resolved: "light" | "dark") {
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    await getCurrentWindow().setTheme(resolved);
    // Theme is app-wide on macOS; refresh every overlay that may be alive.
    await Promise.all([
      invoke("sync_main_panel_appearance").catch(() => undefined),
      invoke("sync_shelf_picker_appearance").catch(() => undefined),
    ]);
  } catch {
    // Some auxiliary windows may not support setTheme; ignore.
  }
}

export async function emitThemeChange(theme: Settings["theme"]) {
  await applyThemeAsync(theme);
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

/**
 * Watch OS appearance while the setting is "system".
 *
 * Prefer the Rust `os:appearance-changed` push (one native poller, emit only on change).
 * matchMedia / onThemeChanged remain as best-effort backups; forcing an explicit window
 * theme for overlay sync often stops them from tracking the OS.
 */
export function watchSystemTheme(
  getTheme: () => Settings["theme"],
  onSystemChange: () => void
): () => void {
  let lastDark: boolean | null = null;
  let cancelled = false;

  const notifyIfChanged = (isDark: boolean) => {
    if (cancelled || getTheme() !== "system") return;
    if (lastDark === null) {
      lastDark = isDark;
      return;
    }
    if (lastDark !== isDark) {
      lastDark = isDark;
      onSystemChange();
    }
  };

  // One-shot seed so the first OS event is compared correctly (not treated as "init").
  void resolveSystemIsDark().then((isDark) => {
    if (!cancelled && lastDark === null) lastDark = isDark;
  });

  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const mediaHandler = () => {
    if (getTheme() === "system") onSystemChange();
  };
  media.addEventListener("change", mediaHandler);

  let unlistenTauri: (() => void) | undefined;
  let unlistenAppearance: (() => void) | undefined;

  if ("__TAURI_INTERNALS__" in window) {
    void getCurrentWindow()
      .onThemeChanged(() => {
        if (getTheme() === "system") onSystemChange();
      })
      .then((fn) => {
        unlistenTauri = fn;
      })
      .catch(() => {
        // Older runtimes / unsupported windows.
      });

    void listen<{ dark: boolean }>(SYSTEM_APPEARANCE_CHANGED_EVENT, (event) => {
      notifyIfChanged(event.payload.dark);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlistenAppearance = fn;
    });
  }

  return () => {
    cancelled = true;
    media.removeEventListener("change", mediaHandler);
    unlistenTauri?.();
    unlistenAppearance?.();
  };
}
