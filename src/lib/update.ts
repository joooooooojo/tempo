import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type UpdateProgress = {
  phase: "idle" | "checking" | "downloading" | "ready" | "installing" | "done";
  downloaded: number;
  total: number;
  version?: string;
};

export type PreparedUpdate = {
  version: string;
};

export type UpdateCheckResult =
  | { status: "latest" }
  | { status: "ready"; version: string; update: PreparedUpdate };

type StagedUpdateResult = {
  status: "idle" | "latest" | "available" | "ready";
  current_version: string;
  version?: string | null;
  pending_version?: string | null;
  active_version?: string | null;
  notes?: string | null;
};

type StagedUpdateProgress = {
  phase: UpdateProgress["phase"];
  downloaded: number;
  total: number;
  version: string;
};

export async function getAppVersion() {
  return getVersion();
}

export async function getPreparedUpdate(): Promise<PreparedUpdate | null> {
  const status = await invoke<StagedUpdateResult>("staged_update_status");
  const version = status.pending_version ?? (status.status === "ready" ? status.version : null);
  return version ? { version } : null;
}

/** Check, download, verify, and stage the update while the current app keeps running. */
export async function checkAndDownloadUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<UpdateCheckResult> {
  onProgress?.({ phase: "checking", downloaded: 0, total: 0 });

  const check = await invoke<StagedUpdateResult>("staged_check_update");
  if (check.status === "latest") {
    onProgress?.({ phase: "idle", downloaded: 0, total: 0 });
    return { status: "latest" };
  }

  const readyVersion = check.pending_version ?? (check.status === "ready" ? check.version : null);
  if (readyVersion) {
    onProgress?.({ phase: "ready", downloaded: 0, total: 0, version: readyVersion });
    return { status: "ready", version: readyVersion, update: { version: readyVersion } };
  }

  const unlisten = await listen<StagedUpdateProgress>("staged-update-progress", (event) => {
    onProgress?.({
      phase: event.payload.phase,
      downloaded: event.payload.downloaded,
      total: event.payload.total,
      version: event.payload.version,
    });
  });

  try {
    const result = await invoke<StagedUpdateResult>("staged_download_update");
    const version = result.pending_version ?? result.version;
    if (!version) {
      onProgress?.({ phase: "idle", downloaded: 0, total: 0 });
      return { status: "latest" };
    }

    onProgress?.({ phase: "ready", downloaded: 0, total: 0, version });
    return { status: "ready", version, update: { version } };
  } finally {
    unlisten();
  }
}

/** Restart into an already-staged version; no installer runs at this point. */
export async function installAndRelaunch(
  update: PreparedUpdate,
  onProgress?: (progress: UpdateProgress) => void,
) {
  onProgress?.({
    phase: "installing",
    downloaded: 0,
    total: 0,
    version: update.version,
  });
  await new Promise<void>((resolve) => window.setTimeout(resolve, 300));
  await invoke<void>("staged_restart_to_update");
}
