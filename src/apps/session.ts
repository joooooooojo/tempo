import { getApp } from "@/apps/registry";

const STORAGE_KEY = "tempo.palette.session.v1";

export interface PaletteSession {
  /** Builtin / plugin app id to restore. */
  appId: string;
  /** Optional opaque payload for plugins (e.g. route, draft id). */
  payload?: Record<string, unknown>;
  updatedAt: string;
}

export interface PaletteSessionStore {
  load: () => PaletteSession | null;
  save: (session: PaletteSession) => void;
  clear: () => void;
}

const localStorageStore: PaletteSessionStore = {
  load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as PaletteSession;
      if (!parsed?.appId || typeof parsed.appId !== "string") return null;
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
    } catch {
      // ignore
    }
  },
};

let store: PaletteSessionStore = localStorageStore;

/** Swap storage backend (e.g. plugin host / sync). */
export function setPaletteSessionStore(next: PaletteSessionStore) {
  store = next;
}

export function getPaletteSessionStore(): PaletteSessionStore {
  return store;
}

export function readPaletteSession(): PaletteSession | null {
  return store.load();
}

export function writePaletteSession(appId: string, payload?: Record<string, unknown>) {
  store.save({
    appId,
    payload,
    updatedAt: new Date().toISOString(),
  });
}

export function clearPaletteSession() {
  store.clear();
}

/** Returns a restorable session only if the app still exists and opted into persistence. */
export function resolveRestorablePaletteSession(): PaletteSession | null {
  const session = readPaletteSession();
  if (!session) return null;
  const app = getApp(session.appId);
  if (!app?.persistSession) {
    clearPaletteSession();
    return null;
  }
  return session;
}

export function canPersistAppSession(appId: string | null | undefined): boolean {
  if (!appId) return false;
  return Boolean(getApp(appId)?.persistSession);
}
