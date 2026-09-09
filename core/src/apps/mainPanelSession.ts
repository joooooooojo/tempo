import { getApp } from "@/apps/registry";

const STORAGE_KEY = "tempo.main-panel.session.v1";
const LEGACY_STORAGE_KEY = "tempo.palette.session.v1";

export interface MainPanelSession {
  /** Builtin / plugin app id to restore. */
  appId: string;
  /** Optional opaque payload for plugins (e.g. route, draft id). */
  payload?: Record<string, unknown>;
  updatedAt: string;
}

export interface MainPanelSessionStore {
  load: () => MainPanelSession | null;
  save: (session: MainPanelSession) => void;
  clear: () => void;
}

const localStorageStore: MainPanelSessionStore = {
  load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as MainPanelSession;
      if (!parsed?.appId || typeof parsed.appId !== "string") return null;
      localStorage.setItem(STORAGE_KEY, raw);
      localStorage.removeItem(LEGACY_STORAGE_KEY);
      return parsed;
    } catch {
      return null;
    }
  },
  save(session) {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
    } catch {
      // ignore quota / private mode
    }
  },
  clear() {
    try {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.removeItem(LEGACY_STORAGE_KEY);
    } catch {
      // ignore
    }
  },
};

let store: MainPanelSessionStore = localStorageStore;

/** Swap storage backend (e.g. plugin host / sync). */
export function setMainPanelSessionStore(next: MainPanelSessionStore) {
  store = next;
}

export function getMainPanelSessionStore(): MainPanelSessionStore {
  return store;
}

export function readMainPanelSession(): MainPanelSession | null {
  return store.load();
}

export function writeMainPanelSession(appId: string, payload?: Record<string, unknown>) {
  store.save({
    appId,
    payload,
    updatedAt: new Date().toISOString(),
  });
}

export function clearMainPanelSession() {
  store.clear();
}

/** After this long without reopening, do not restore the last plugin — show search. */
export const MAIN_PANEL_APP_SESSION_STALE_MS = 60_000;

function isSessionStale(session: MainPanelSession, now = Date.now()): boolean {
  const updatedAt = Date.parse(session.updatedAt);
  if (!Number.isFinite(updatedAt)) return true;
  return now - updatedAt >= MAIN_PANEL_APP_SESSION_STALE_MS;
}

/** Returns a restorable session while the referenced app still exists and is fresh. */
export function resolveRestorableMainPanelSession(): MainPanelSession | null {
  const session = readMainPanelSession();
  if (!session) return null;
  if (isSessionStale(session)) {
    clearMainPanelSession();
    return null;
  }
  const app = getApp(session.appId);
  if (!app) {
    clearMainPanelSession();
    return null;
  }
  return session;
}
