import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAuxiliaryWindowShell } from "@/hooks/useAuxiliaryWindow";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type {
  LauncherContextMenuItem,
  LauncherContextMenuPayload,
  LauncherContextMenuTarget,
} from "@/lib/launcherContextMenu";

export function LauncherContextMenuPage() {
  useAuxiliaryWindowShell("launcher-context-menu-window");

  const [items, setItems] = useState<LauncherContextMenuItem[]>([]);
  const [target, setTarget] = useState<LauncherContextMenuTarget | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const applyPayload = (payload: LauncherContextMenuPayload) => {
      setItems(payload.items ?? []);
      setTarget(payload.target);
      setReady(true);
    };

    const unlistenPrepare = listen<LauncherContextMenuPayload>(
      "launcher-context-menu:prepare",
      (event) => applyPayload(event.payload),
    );
    const unlistenOpen = listen<LauncherContextMenuPayload>(
      "launcher-context-menu:open",
      (event) => applyPayload(event.payload),
    );
    const unlistenHide = listen("launcher-context-menu:hide", () => {
      setReady(false);
      setItems([]);
      setTarget(null);
    });

    const appWindow = getCurrentWindow();
    let armed = false;
    let armTimer = 0;
    const armBlurClose = () => {
      window.clearTimeout(armTimer);
      armTimer = window.setTimeout(() => {
        armed = true;
      }, 120);
    };

    void unlistenOpen.then(() => armBlurClose());

    let unlistenBlur: (() => void) | undefined;
    void appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused && armed) {
          void api.hideLauncherContextMenu("blur");
        }
      })
      .then((fn) => {
        unlistenBlur = fn;
      });

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      void api.hideLauncherContextMenu("escape");
    };
    window.addEventListener("keydown", onKeyDown);

    // Prevent the native WebView menu inside this window as well.
    const onContextMenu = (event: Event) => event.preventDefault();
    document.addEventListener("contextmenu", onContextMenu, true);

    armBlurClose();

    return () => {
      window.clearTimeout(armTimer);
      window.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("contextmenu", onContextMenu, true);
      void unlistenPrepare.then((fn) => fn());
      void unlistenOpen.then((fn) => fn());
      void unlistenHide.then((fn) => fn());
      unlistenBlur?.();
    };
  }, []);

  const onSelect = (item: LauncherContextMenuItem) => {
    if (item.disabled || !target) return;
    void api.launcherContextMenuAction(item.id, target);
  };

  return (
    <div
      className="launcher-context-menu-page"
      data-ready={ready ? "true" : "false"}
      onContextMenu={(event) => event.preventDefault()}
    >
      <div className="launcher-context-menu" role="menu" aria-label="应用操作">
        {items.map((item) => (
          <div key={item.id} className="launcher-context-menu__block">
            {item.separatorBefore ? (
              <div className="launcher-context-menu__separator" role="separator" />
            ) : null}
            <button
              type="button"
              role="menuitem"
              className={cn(
                "launcher-context-menu__item",
                item.danger && "launcher-context-menu__item--danger",
              )}
              disabled={item.disabled}
              onClick={() => onSelect(item)}
              onMouseDown={(event) => {
                // Keep focus on the menu window until the click completes.
                event.preventDefault();
              }}
            >
              {item.label}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
