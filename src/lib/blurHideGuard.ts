/** Suppress command-palette / shelf auto-hide while a native dialog holds focus. */

let suppressDepth = 0;

export function isBlurHideSuppressed(): boolean {
  return suppressDepth > 0;
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
