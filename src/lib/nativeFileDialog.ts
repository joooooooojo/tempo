import { invoke } from "@tauri-apps/api/core";
import { open, type OpenDialogOptions } from "@tauri-apps/plugin-dialog";
import { withBlurHideSuppressed } from "@/lib/blurHideGuard";

function isWindowsHost(): boolean {
  return /Windows/i.test(navigator.userAgent);
}

export function normalizeNativePath(value: string): string {
  let normalized = value;

  if (/^file:/i.test(normalized)) {
    try {
      const url = new URL(normalized);
      const pathname = decodeURIComponent(url.pathname);
      if (isWindowsHost()) {
        const networkHost =
          url.host && url.host !== "localhost" ? url.host : "";
        normalized = networkHost
          ? `\\\\${networkHost}${pathname.replace(/\//g, "\\")}`
          : pathname.replace(/^\/([A-Za-z]:)/, "$1").replace(/\//g, "\\");
      } else {
        normalized =
          url.host && url.host !== "localhost"
            ? `//${url.host}${pathname}`
            : pathname;
      }
    } catch {
      return value;
    }
  }

  if (normalized.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${normalized.slice(8)}`;
  }
  if (normalized.startsWith("\\\\?\\")) {
    return normalized.slice(4);
  }
  return normalized;
}

function normalizeDialogResult(
  result: string | string[] | null,
): string | string[] | null {
  if (Array.isArray(result)) return result.map(normalizeNativePath);
  return typeof result === "string" ? normalizeNativePath(result) : result;
}

/**
 * Native file/folder picker for overlay windows (main panel / shelf).
 * Matches ZTools: parent dialogs to the overlay at modal-panel level, and suppress
 * blur→hide so opening NSOpenPanel does not dismiss the main panel.
 */
export async function openNativeFileDialog(
  options: OpenDialogOptions,
): Promise<string | string[] | null> {
  return withBlurHideSuppressed(async () => {
    try {
      await invoke("prepare_native_file_dialog");
    } catch {
      // Non-fatal: still attempt the dialog for a non-overlay window or non-macOS host.
    }
    try {
      return normalizeDialogResult(await open(options));
    } finally {
      try {
        await invoke("restore_after_native_file_dialog");
      } catch {
        // ignore
      }
    }
  });
}
