import { invoke } from "@tauri-apps/api/core";
import { open, type OpenDialogOptions } from "@tauri-apps/plugin-dialog";
import { withBlurHideSuppressed } from "@/lib/blurHideGuard";

/**
 * Native file/folder picker for overlay windows (main panel / shelf).
 * Matches ZTools: parent dialogs to the overlay at modal-panel level, and suppress
 * blur→hide so opening NSOpenPanel does not dismiss the main panel.
 */
export async function openNativeFileDialog(
  options: OpenDialogOptions
): Promise<string | string[] | null> {
  return withBlurHideSuppressed(async () => {
    try {
      await invoke("prepare_native_file_dialog");
    } catch {
      // Non-fatal: still attempt the dialog for a non-overlay window or non-macOS host.
    }
    try {
      return await open(options);
    } finally {
      try {
        await invoke("restore_after_native_file_dialog");
      } catch {
        // ignore
      }
    }
  });
}
