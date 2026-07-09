import { getVersion } from "@tauri-apps/api/app";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "ready" | "installing" | "done";
  downloaded: number;
  total: number;
  version?: string;
};

export type UpdateCheckResult =
  | { status: "latest" }
  | { status: "ready"; version: string; update: Update };

export async function getAppVersion() {
  return getVersion();
}

/** Check and download only. Keep the package ready until the user restarts. */
export async function checkAndDownloadUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<UpdateCheckResult> {
  onProgress?.({ phase: "checking", downloaded: 0, total: 0 });

  const update = await check();
  if (!update) {
    onProgress?.({ phase: "idle", downloaded: 0, total: 0 });
    return { status: "latest" };
  }

  let downloaded = 0;
  let total = 0;

  await update.download((event: DownloadEvent) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? 0;
        onProgress?.({
          phase: "downloading",
          downloaded: 0,
          total,
          version: update.version,
        });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({
          phase: "downloading",
          downloaded,
          total,
          version: update.version,
        });
        break;
      case "Finished":
        onProgress?.({
          phase: "ready",
          downloaded,
          total,
          version: update.version,
        });
        break;
    }
  });

  onProgress?.({
    phase: "ready",
    downloaded,
    total,
    version: update.version,
  });

  return { status: "ready", version: update.version, update };
}

/** Install the downloaded package (Windows exits here), then relaunch. */
export async function installAndRelaunch(
  update: Update,
  onProgress?: (progress: UpdateProgress) => void,
) {
  onProgress?.({
    phase: "installing",
    downloaded: 0,
    total: 0,
    version: update.version,
  });

  await update.install();
  onProgress?.({
    phase: "done",
    downloaded: 0,
    total: 0,
    version: update.version,
  });
  await relaunch();
}
