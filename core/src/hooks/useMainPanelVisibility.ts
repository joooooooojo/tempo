import { useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { MainPanelHiddenPayload, MainPanelShownPayload } from "@/types";

interface UseMainPanelVisibilityOptions {
  enabled: boolean;
  /** A new visible session started (generation strictly increases). */
  onShown: (payload: MainPanelShownPayload) => void;
  /**
   * The panel was hidden. `reason === "command"` means this webview asked for
   * it and already did its own bookkeeping; other reasons are external
   * (app deactivated, shortcut, plugin) and usually preserve the session.
   */
  onHidden: (payload: MainPanelHiddenPayload) => void;
}

/**
 * Mirror of the Rust main panel controller. Rust owns visibility; this hook
 * only tracks the current generation so hide requests cannot close a newer
 * session, and replays the show that may have happened before the listener
 * was registered (startup race).
 */
export function useMainPanelVisibility({
  enabled,
  onShown,
  onHidden,
}: UseMainPanelVisibilityOptions) {
  const generationRef = useRef(0);
  const visibleRef = useRef(false);
  const onShownRef = useRef(onShown);
  const onHiddenRef = useRef(onHidden);
  onShownRef.current = onShown;
  onHiddenRef.current = onHidden;

  useEffect(() => {
    if (!enabled) return;
    let disposed = false;

    const applyShown = (payload: MainPanelShownPayload) => {
      if (disposed || payload.generation <= generationRef.current) return;
      generationRef.current = payload.generation;
      visibleRef.current = true;
      onShownRef.current(payload);
    };

    const unlistenShown = listen<MainPanelShownPayload>("main-panel:shown", (event) => {
      applyShown(event.payload);
    });
    const unlistenHidden = listen<MainPanelHiddenPayload>("main-panel:hidden", (event) => {
      if (disposed) return;
      visibleRef.current = false;
      onHiddenRef.current(event.payload);
    });

    // The startup show can fire before this WebView registers its listener.
    void unlistenShown
      .then(() => api.getMainPanelState())
      .then((state) => {
        if (state.visible) applyShown({ generation: state.generation, reason: "startup" });
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      void unlistenShown.then((unlisten) => unlisten());
      void unlistenHidden.then((unlisten) => unlisten());
    };
  }, [enabled]);

  /** Hide the current session. Resolves `false` when a newer session already replaced it. */
  const hide = useCallback(async (): Promise<boolean> => {
    if (!enabled) return false;
    return api.hideMainPanel(generationRef.current).catch(() => false);
  }, [enabled]);

  return { generationRef, visibleRef, hide };
}
