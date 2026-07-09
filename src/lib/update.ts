import { getVersion } from "@tauri-apps/api/app";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "installing" | "done";
  downloaded: number;
  total: number;
};

export async function getAppVersion() {
  return getVersion();
}

export async function checkForAppUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<"latest" | "updated"> {
  onProgress?.({ phase: "checking", downloaded: 0, total: 0 });

  const update = await check();
  if (!update) {
    onProgress?.({ phase: "idle", downloaded: 0, total: 0 });
    return "latest";
  }

  let downloaded = 0;
  let total = 0;

  await update.downloadAndInstall((event: DownloadEvent) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? 0;
        onProgress?.({ phase: "downloading", downloaded: 0, total });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({ phase: "downloading", downloaded, total });
        break;
      case "Finished":
        onProgress?.({ phase: "installing", downloaded, total });
        break;
    }
  });

  onProgress?.({ phase: "done", downloaded, total });
  await relaunch();
  return "updated";
}
