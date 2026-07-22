import { BUILTIN_QUICK_ACTIONS } from "@/apps/actions/builtin";
import { BUILTIN_OWNER } from "@/apps/constants";
import type { QuickAction, Registration } from "@/apps/types";

export const ACTION_USAGE_PREFIX = "action:";

type ActionListener = () => void;

const actions: QuickAction[] = [];
const byId = new Map<string, QuickAction>();
const ownerById = new Map<string, string>();
const listeners = new Set<ActionListener>();

function emit() {
  for (const listener of listeners) listener();
}

export function quickActionUsageId(actionId: string) {
  return `${ACTION_USAGE_PREFIX}${actionId}`;
}

export function listQuickActions(): QuickAction[] {
  return actions.slice();
}

export function getQuickAction(id: string): QuickAction | undefined {
  return byId.get(id);
}

export type QuickActionUsageHint = {
  last_used_at?: string | null;
  use_count: number;
};

function usageTimeMs(value: string | null | undefined): number {
  if (!value) return 0;
  const ms = Date.parse(value);
  return Number.isFinite(ms) ? ms : 0;
}

/**
 * Actions visible for the current search query, sorted by last use (then use count).
 * Unused actions keep registration order after used ones.
 */
export function listVisibleQuickActions(
  query: string,
  usageById?: Map<string, QuickActionUsageHint>
): QuickAction[] {
  const normalized = query.trim();
  const visible = listQuickActions().filter((action) => {
    if (action.requiresQuery !== false && !normalized) return false;
    return true;
  });

  if (!usageById || usageById.size === 0) return visible;

  return visible
    .map((action, index) => ({ action, index }))
    .sort((left, right) => {
      const leftUsage = usageById.get(quickActionUsageId(left.action.id));
      const rightUsage = usageById.get(quickActionUsageId(right.action.id));
      const leftTime = usageTimeMs(leftUsage?.last_used_at);
      const rightTime = usageTimeMs(rightUsage?.last_used_at);
      if (rightTime !== leftTime) return rightTime - leftTime;
      const leftCount = leftUsage?.use_count ?? 0;
      const rightCount = rightUsage?.use_count ?? 0;
      if (rightCount !== leftCount) return rightCount - leftCount;
      return left.index - right.index;
    })
    .map((entry) => entry.action);
}

/**
 * Register a quick action. Duplicate ids from a different owner are rejected.
 * Same-owner re-register replaces in place (dev reload).
 */
export function registerQuickAction(
  action: QuickAction,
  ownerPluginId: string = action.pluginId ?? (action.source === "builtin" ? BUILTIN_OWNER : action.id)
): Registration {
  const existingOwner = ownerById.get(action.id);
  if (existingOwner && existingOwner !== ownerPluginId) {
    throw new Error(
      `Action id "${action.id}" is already owned by "${existingOwner}"; cannot register as "${ownerPluginId}"`
    );
  }

  const existingIndex = actions.findIndex((item) => item.id === action.id);
  if (existingIndex >= 0) {
    actions[existingIndex] = action;
  } else {
    actions.push(action);
  }
  byId.set(action.id, action);
  ownerById.set(action.id, ownerPluginId);
  emit();

  return {
    dispose() {
      if (ownerById.get(action.id) !== ownerPluginId) return;
      const index = actions.findIndex((item) => item.id === action.id);
      if (index >= 0) actions.splice(index, 1);
      byId.delete(action.id);
      ownerById.delete(action.id);
      emit();
    },
  };
}

export function unregisterQuickAction(id: string, ownerPluginId?: string): boolean {
  if (ownerPluginId && ownerById.get(id) !== ownerPluginId) {
    return false;
  }
  const index = actions.findIndex((item) => item.id === id);
  if (index < 0) return false;
  actions.splice(index, 1);
  byId.delete(id);
  ownerById.delete(id);
  emit();
  return true;
}

export function unregisterAllActions(ownerPluginId: string): void {
  const removeIds = [...ownerById.entries()]
    .filter(([, owner]) => owner === ownerPluginId)
    .map(([id]) => id);
  if (removeIds.length === 0) return;
  for (const id of removeIds) {
    const index = actions.findIndex((item) => item.id === id);
    if (index >= 0) actions.splice(index, 1);
    byId.delete(id);
    ownerById.delete(id);
  }
  emit();
}

export function subscribeQuickActions(listener: ActionListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

for (const action of BUILTIN_QUICK_ACTIONS) {
  registerQuickAction(action, BUILTIN_OWNER);
}
