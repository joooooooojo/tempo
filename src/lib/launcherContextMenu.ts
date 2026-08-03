import type { MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setContextMenuBlurHideSuppressed } from "@/lib/blurHideGuard";
import { api } from "@/lib/api";

export type LauncherContextMenuTarget =
  | {
      kind: "launcher";
      id: string;
      name: string;
      pinned: boolean;
      /** True when opened from the「最近使用」section. */
      fromRecent?: boolean;
    }
  | {
      kind: "contribution";
      id: string;
      name: string;
      pinned: boolean;
      source: "builtin" | "plugin";
      fromRecent?: boolean;
    };

export type LauncherContextMenuItem = {
  id: string;
  label: string;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
};

export type LauncherContextMenuPayload = {
  items: LauncherContextMenuItem[];
  target: LauncherContextMenuTarget;
};

export type LauncherContextMenuAction = {
  actionId: string;
  target: LauncherContextMenuTarget;
};

export type LauncherContextMenuClosed = {
  reason: "action" | "blur" | "escape" | "dismiss";
};

export function buildTileContextMenuItems(
  target: LauncherContextMenuTarget,
): LauncherContextMenuItem[] {
  const items: LauncherContextMenuItem[] = [{ id: "open", label: "打开" }];

  // Always show「打开位置」; builtin contributions have no filesystem path.
  items.push({
    id: "open-location",
    label: "打开位置",
    disabled: target.kind === "contribution" && target.source === "builtin",
  });

  if (target.fromRecent) {
    items.push({
      id: "remove-recent",
      label: "移出列表",
      separatorBefore: true,
    });
  }

  items.push({
    id: "toggle-pin",
    label: target.pinned ? "取消固定" : "固定",
    separatorBefore: !target.fromRecent,
  });

  return items;
}

export function usageIdForContextTarget(target: LauncherContextMenuTarget): string {
  if (target.kind === "launcher") return target.id;
  return target.source === "plugin" ? `plugin:${target.id}` : `builtin:${target.id}`;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Open the floating launcher context-menu window at the cursor. */
export async function openLauncherContextMenu(
  event: MouseEvent,
  target: LauncherContextMenuTarget,
): Promise<void> {
  event.preventDefault();
  event.stopPropagation();
  if (!isTauriRuntime()) return;

  setContextMenuBlurHideSuppressed(true);
  // Backup: if the menu never emits closed (or show hangs without freezing JS),
  // don't leave main-panel blur-hide permanently suppressed.
  window.setTimeout(() => {
    setContextMenuBlurHideSuppressed(false);
  }, 8000);

  try {
    const win = getCurrentWindow();
    const [factor, pos] = await Promise.all([win.scaleFactor(), win.innerPosition()]);
    const x = pos.x + Math.round(event.clientX * factor);
    const y = pos.y + Math.round(event.clientY * factor);
    await api.showLauncherContextMenu({
      x,
      y,
      items: buildTileContextMenuItems(target),
      target,
    });
  } catch (error) {
    setContextMenuBlurHideSuppressed(false);
    throw error;
  }
}
