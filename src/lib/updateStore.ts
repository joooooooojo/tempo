import { useSyncExternalStore } from "react";
import {
  checkUpdate,
  downloadInstallAndRelaunch,
  getAppVersion,
  installAndRelaunch,
  type AvailableUpdate,
  type UpdateProgress,
} from "@/lib/update";

const STORAGE_KEY = "tempo:pending-update";

type PersistedPendingUpdate = {
  version: string;
  readyAt: number;
};

export type UpdateStoreState = {
  checking: boolean;
  applying: boolean;
  progress: UpdateProgress | null;
  pendingUpdate: AvailableUpdate | null;
  pendingVersion: string;
};

type Listener = () => void;

const listeners = new Set<Listener>();

function readPersisted(): PersistedPendingUpdate | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PersistedPendingUpdate;
    if (!parsed || typeof parsed.version !== "string" || !parsed.version.trim()) {
      return null;
    }
    return { version: parsed.version.trim(), readyAt: Number(parsed.readyAt) || Date.now() };
  } catch {
    return null;
  }
}

const initialPersisted = typeof localStorage !== "undefined" ? readPersisted() : null;

let state: UpdateStoreState = {
  checking: false,
  applying: false,
  progress: initialPersisted
    ? {
        phase: "ready",
        downloaded: 0,
        total: 0,
        version: initialPersisted.version,
      }
    : null,
  pendingUpdate: null,
  pendingVersion: initialPersisted?.version ?? "",
};

let hydrated = false;
let hydratePromise: Promise<void> | null = null;

function emit() {
  for (const listener of listeners) listener();
}

function setState(patch: Partial<UpdateStoreState>) {
  state = { ...state, ...patch };
  emit();
}

function writePersisted(version: string) {
  const payload: PersistedPendingUpdate = { version, readyAt: Date.now() };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
}

function clearPersisted() {
  localStorage.removeItem(STORAGE_KEY);
}

function compareVersions(a: string, b: string): number {
  const pa = a.split(/[.+-]/).map((part) => Number.parseInt(part, 10) || 0);
  const pb = b.split(/[.+-]/).map((part) => Number.parseInt(part, 10) || 0);
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i += 1) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

async function hydrateFromStorage() {
  if (hydrated) return;
  hydrated = true;

  const persisted = readPersisted();
  if (!persisted) {
    if (state.pendingVersion && !state.pendingUpdate) {
      setState({ pendingVersion: "", progress: null });
    }
    return;
  }

  try {
    const current = await getAppVersion();
    if (compareVersions(persisted.version, current) <= 0) {
      clearPersisted();
      setState({ pendingVersion: "", progress: null, pendingUpdate: null });
      return;
    }
    if (state.pendingVersion !== persisted.version) {
      setState({
        pendingVersion: persisted.version,
        progress: {
          phase: "ready",
          downloaded: 0,
          total: 0,
          version: persisted.version,
        },
      });
    }
  } catch {
    // Keep already-hydrated metadata from localStorage.
  }
}

export function ensureUpdateStoreHydrated() {
  if (!hydratePromise) {
    hydratePromise = hydrateFromStorage();
  }
  return hydratePromise;
}

export function getUpdateStoreState() {
  return state;
}

export function subscribeUpdateStore(listener: Listener) {
  listeners.add(listener);
  void ensureUpdateStoreHydrated();
  return () => {
    listeners.delete(listener);
  };
}

export function useUpdateStore() {
  return useSyncExternalStore(subscribeUpdateStore, getUpdateStoreState, getUpdateStoreState);
}

export async function runCheckUpdate(): Promise<
  { status: "latest" } | { status: "ready"; version: string } | { status: "busy" }
> {
  await ensureUpdateStoreHydrated();

  if (state.checking || state.applying) return { status: "busy" };
  if (state.pendingUpdate) return { status: "ready", version: state.pendingUpdate.version };

  setState({
    checking: true,
    progress: { phase: "checking", downloaded: 0, total: 0 },
  });

  try {
    const result = await checkUpdate((progress) => {
      setState({ progress });
    });

    if (result.status === "latest") {
      clearPersisted();
      setState({
        pendingUpdate: null,
        pendingVersion: "",
        progress: null,
      });
      return { status: "latest" };
    }

    writePersisted(result.version);
    setState({
      pendingUpdate: result.update,
      pendingVersion: result.version,
      progress: {
        phase: "ready",
        downloaded: state.progress?.downloaded ?? 0,
        total: state.progress?.total ?? 0,
        version: result.version,
      },
    });
    return { status: "ready", version: result.version };
  } catch (error) {
    setState({ progress: null });
    throw error;
  } finally {
    setState({ checking: false });
  }
}

export async function runInstallUpdate(): Promise<"installed" | "latest" | "busy"> {
  await ensureUpdateStoreHydrated();

  if (state.applying || state.checking) return "busy";
  if (!state.pendingUpdate && !state.pendingVersion) return "busy";

  const version = state.pendingVersion || state.pendingUpdate?.version || "";
  setState({
    applying: true,
    progress: {
      phase: state.pendingUpdate ? "installing" : "checking",
      downloaded: 0,
      total: 0,
      version,
    },
  });

  try {
    if (state.pendingUpdate) {
      await installAndRelaunch(state.pendingUpdate, (progress) => {
        setState({ progress });
      });
      clearPersisted();
      return "installed";
    }

    const outcome = await downloadInstallAndRelaunch((progress) => {
      setState({ progress });
    });

    if (outcome === "latest") {
      clearPersisted();
      setState({
        pendingUpdate: null,
        pendingVersion: "",
        progress: null,
        applying: false,
      });
      return "latest";
    }

    clearPersisted();
    return "installed";
  } catch (error) {
    setState({
      applying: false,
      progress: {
        phase: "ready",
        downloaded: 0,
        total: 0,
        version,
      },
    });
    throw error;
  }
}
