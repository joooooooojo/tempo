import { emit, listen } from "@tauri-apps/api/event";

export const MAIN_PANEL_ICON_CHANGED_EVENT = "settings:main-panel-icon-changed";

export async function emitMainPanelIconChange(dataUrl: string) {
  await emit(MAIN_PANEL_ICON_CHANGED_EVENT, { dataUrl });
}

export function subscribeMainPanelIconChanges(
  onChange: (dataUrl: string) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;

  void listen<{ dataUrl: string }>(MAIN_PANEL_ICON_CHANGED_EVENT, (event) => {
    if (!disposed) onChange(event.payload.dataUrl);
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
