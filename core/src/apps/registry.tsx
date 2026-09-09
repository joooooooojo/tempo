import type { Registration, TempoApp } from "@/apps/types";

export { BUILTIN_OWNER } from "@/apps/constants";

type AppListener = () => void;

const apps: TempoApp[] = [];
const byId = new Map<string, TempoApp>();
const ownerById = new Map<string, string>();
const listeners = new Set<AppListener>();

function emit() {
  for (const listener of listeners) listener();
}

function assertOwner(ownerPluginId: string, appId: string) {
  const existingOwner = ownerById.get(appId);
  if (existingOwner && existingOwner !== ownerPluginId) {
    throw new Error(
      `App id "${appId}" is already owned by "${existingOwner}"; cannot register as "${ownerPluginId}"`,
    );
  }
}

export function registerApp(
  ownerPluginId: string,
  app: TempoApp,
): Registration {
  assertOwner(ownerPluginId, app.id);
  if (byId.has(app.id) && ownerById.get(app.id) !== ownerPluginId) {
    throw new Error(`Duplicate app id "${app.id}"`);
  }

  const existingIndex = apps.findIndex((item) => item.id === app.id);
  if (existingIndex >= 0) {
    if (ownerById.get(app.id) !== ownerPluginId) {
      throw new Error(
        `Cannot replace app "${app.id}" owned by another registrant`,
      );
    }
    apps[existingIndex] = app;
  } else {
    apps.push(app);
  }
  byId.set(app.id, app);
  ownerById.set(app.id, ownerPluginId);
  emit();

  return {
    dispose() {
      if (ownerById.get(app.id) !== ownerPluginId) return;
      const index = apps.findIndex((item) => item.id === app.id);
      if (index >= 0) apps.splice(index, 1);
      byId.delete(app.id);
      ownerById.delete(app.id);
      emit();
    },
  };
}

export function unregisterAll(ownerPluginId: string): void {
  const removeIds = [...ownerById.entries()]
    .filter(([, owner]) => owner === ownerPluginId)
    .map(([id]) => id);
  if (removeIds.length === 0) return;
  for (const id of removeIds) {
    const index = apps.findIndex((item) => item.id === id);
    if (index >= 0) apps.splice(index, 1);
    byId.delete(id);
    ownerById.delete(id);
  }
  emit();
}

export function getApp(id: string): TempoApp | undefined {
  return byId.get(id);
}

export function listApps(): TempoApp[] {
  return apps.slice();
}

export function subscribeApps(listener: AppListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** @deprecated Prefer getApp */
export function getBuiltinApp(id: string): TempoApp | undefined {
  return getApp(id);
}

/** @deprecated Prefer listApps */
export function listBuiltinApps(): TempoApp[] {
  return listApps().filter((app) => app.source === "builtin");
}
