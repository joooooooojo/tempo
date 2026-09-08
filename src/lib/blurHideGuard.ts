/** Suppress main-panel / shelf auto-hide while a native dialog holds focus. */

let suppressDepth = 0;
let contextMenuSuppressActive = false;
let contextMenuSession = false;
let contextMenuReleaseTimer = 0;
let contextMenuBackupTimer = 0;
let devtoolsSuppressActive = false;

export function isBlurHideSuppressed(): boolean {
  return suppressDepth > 0;
}

/** Increment/decrement blur-hide suppression (e.g. while a floating menu is open). */
export function setBlurHideSuppressed(active: boolean): void {
  if (active) {
    suppressDepth += 1;
    return;
  }
  suppressDepth = Math.max(0, suppressDepth - 1);
}

/**
 * Idempotent suppress while the launcher context-menu window is open.
 * Safe to call repeatedly when reopening the menu without a close event.
 */
export function setContextMenuBlurHideSuppressed(active: boolean): void {
  if (active === contextMenuSuppressActive) return;
  contextMenuSuppressActive = active;
  setBlurHideSuppressed(active);
}

function clearContextMenuTimers(): void {
  if (typeof window === "undefined") return;
  window.clearTimeout(contextMenuReleaseTimer);
  window.clearTimeout(contextMenuBackupTimer);
  contextMenuReleaseTimer = 0;
  contextMenuBackupTimer = 0;
}

/** Right-click started: ignore blur-hide until a menu session begins or this expires. */
export function armContextMenuPointerSuppress(): void {
  setContextMenuBlurHideSuppressed(true);
  if (typeof window === "undefined") return;
  window.clearTimeout(contextMenuReleaseTimer);
  contextMenuReleaseTimer = window.setTimeout(() => {
    if (!contextMenuSession) setContextMenuBlurHideSuppressed(false);
  }, 500);
}

/** Menu is opening: keep blur-hide suppressed until `endContextMenuSession`. */
export function beginContextMenuSession(): void {
  contextMenuSession = true;
  setContextMenuBlurHideSuppressed(true);
  if (typeof window === "undefined") return;
  window.clearTimeout(contextMenuReleaseTimer);
  window.clearTimeout(contextMenuBackupTimer);
  contextMenuBackupTimer = window.setTimeout(() => {
    endContextMenuSession();
  }, 8000);
}

export function endContextMenuSession(): void {
  contextMenuSession = false;
  clearContextMenuTimers();
  setContextMenuBlurHideSuppressed(false);
}

/**
 * Keep the main panel open while WebView DevTools exists (any focus target).
 * Cleared only when DevTools is closed (or the panel is intentionally hidden).
 */
export function setDevtoolsBlurHideSuppressed(active: boolean): void {
  if (active === devtoolsSuppressActive) return;
  devtoolsSuppressActive = active;
  setBlurHideSuppressed(active);
}

export function isDevtoolsBlurHideSuppressed(): boolean {
  return devtoolsSuppressActive;
}

/** Like ZTools `withBlurHideSuppressed` — keep overlays open across NSOpenPanel focus loss. */
export async function withBlurHideSuppressed<T>(fn: () => Promise<T>): Promise<T> {
  suppressDepth += 1;
  try {
    return await fn();
  } finally {
    suppressDepth = Math.max(0, suppressDepth - 1);
  }
}
