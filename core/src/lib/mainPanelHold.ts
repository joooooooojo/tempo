/**
 * Named holds that keep the main panel open across a genuine app deactivation
 * (UAC prompt, native picker, "pin window" in plugin dev mode).
 *
 * The panel auto-hides only when another *application* becomes active (see
 * `core/src-tauri/src/main_panel`). In-app focus changes — context menu, shelf,
 * plugin windows, DevTools, `window.confirm` — never hide it, so most code
 * needs no hold at all.
 */
import { invoke } from "@tauri-apps/api/core";

export interface MainPanelHold {
  /** Resolves once the backend has registered the hold. */
  ready: Promise<void>;
  /** Idempotent. */
  release: () => void;
}

const activeCounts = new Map<string, number>();

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Acquire a hold; nested acquisitions of the same key release together. */
export function acquireMainPanelHold(key: string): MainPanelHold {
  const count = activeCounts.get(key) ?? 0;
  activeCounts.set(key, count + 1);

  const ready =
    count === 0 && isTauriRuntime()
      ? invoke<void>("main_panel_hold", { key }).catch(() => undefined)
      : Promise.resolve();

  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    const current = activeCounts.get(key) ?? 0;
    if (current <= 1) {
      activeCounts.delete(key);
      if (isTauriRuntime()) {
        void ready.then(() =>
          invoke<void>("main_panel_release", { key }).catch(() => undefined),
        );
      }
      return;
    }
    activeCounts.set(key, current - 1);
  };

  return { ready, release };
}

/** Run `fn` while holding the panel open; the hold is registered before `fn` starts. */
export async function withMainPanelHold<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const hold = acquireMainPanelHold(key);
  await hold.ready;
  try {
    return await fn();
  } finally {
    hold.release();
  }
}
