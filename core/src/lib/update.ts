import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "ready" | "installing" | "done";
  downloaded: number;
  total: number;
  version?: string;
};

export type AvailableUpdate = NonNullable<Awaited<ReturnType<typeof check>>>;

export type UpdateCheckResult =
  | { status: "latest" }
  | { status: "ready"; version: string; update: AvailableUpdate };

export async function getAppVersion() {
  return getVersion();
}

export async function checkUpdate(
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

  onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
  await update.download((event) => {
    switch (event.event) {
      case "Started":
        downloaded = 0;
        total = event.data.contentLength ?? 0;
        onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
        break;
      case "Finished":
        onProgress?.({ phase: "ready", downloaded, total, version: update.version });
        break;
    }
  });

  onProgress?.({ phase: "ready", downloaded, total, version: update.version });
  return { status: "ready", version: update.version, update };
}

export async function installAndRelaunch(
  update: AvailableUpdate,
  onProgress?: (progress: UpdateProgress) => void,
) {
  onProgress?.({ phase: "installing", downloaded: 0, total: 0, version: update.version });
  await update.install();

  onProgress?.({ phase: "done", downloaded: 0, total: 0, version: update.version });
  await relaunch();
}

/** Re-check, download, install, and relaunch when the in-memory Update resource is gone. */
export async function downloadInstallAndRelaunch(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<"latest" | "installed"> {
  onProgress?.({ phase: "checking", downloaded: 0, total: 0 });

  const update = await check();
  if (!update) {
    onProgress?.({ phase: "idle", downloaded: 0, total: 0 });
    return "latest";
  }

  let downloaded = 0;
  let total = 0;

  onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        downloaded = 0;
        total = event.data.contentLength ?? 0;
        onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({ phase: "downloading", downloaded, total, version: update.version });
        break;
      case "Finished":
        onProgress?.({
          phase: "installing",
          downloaded,
          total,
          version: update.version,
        });
        break;
    }
  });

  onProgress?.({ phase: "done", downloaded: 0, total: 0, version: update.version });
  await relaunch();
  return "installed";
}
