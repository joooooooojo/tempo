/** Suppress main-panel / shelf auto-hide while a native dialog holds focus. */

let suppressDepth = 0;
let contextMenuSuppressActive = false;
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
