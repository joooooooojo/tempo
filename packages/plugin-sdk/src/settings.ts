import { SETTINGS_CHANGED_EVENT, TEMPO_SETTINGS_KEY } from "./constants.js";
import type { PluginStorageApi } from "./storage.js";
import type { Unsubscribe } from "./types.js";

export interface SettingsChangedPayload {
  values: Record<string, unknown>;
}

export interface PluginSettingsApi {
  /** Reserved storage key (`__tempo/settings`). */
  readonly key: typeof TEMPO_SETTINGS_KEY;
  /** Host event name (`settings.changed`). */
  readonly changedEvent: typeof SETTINGS_CHANGED_EVENT;
  /** Full settings object (empty object when unset). */
  getAll<T extends Record<string, unknown> = Record<string, unknown>>(): Promise<T>;
  /** Read one setting id. */
  get<T = unknown>(id: string): Promise<T | undefined>;
  /** Read one setting id with a fallback. */
  get<T>(id: string, defaultValue: T): Promise<T>;
  /**
   * Subscribe to host-driven updates.
   * UI: pushed when the user saves in「插件配置」.
   * Runtime: pushed when the plugin process is running.
   */
  subscribe(handler: (values: Record<string, unknown>) => void): Unsubscribe;
}

type EventOn = (event: string, handler: (payload: unknown) => void) => Unsubscribe;

export function createSettingsApi(
  storage: PluginStorageApi,
  onEvent: EventOn
): PluginSettingsApi {
  async function getAll<T extends Record<string, unknown> = Record<string, unknown>>(): Promise<T> {
    const value = await storage.get<Record<string, unknown>>(TEMPO_SETTINGS_KEY);
    return { ...(value ?? {}) } as T;
  }

  async function get<T = unknown>(id: string, defaultValue?: T): Promise<T | undefined> {
    const all = await getAll();
    if (Object.prototype.hasOwnProperty.call(all, id)) {
      return all[id] as T;
    }
    return defaultValue;
  }

  return {
    key: TEMPO_SETTINGS_KEY,
    changedEvent: SETTINGS_CHANGED_EVENT,
    getAll,
    get,
    subscribe(handler) {
      return onEvent(SETTINGS_CHANGED_EVENT, (payload) => {
        const values =
          payload &&
          typeof payload === "object" &&
          payload !== null &&
          "values" in payload &&
          payload.values &&
          typeof payload.values === "object"
            ? (payload.values as Record<string, unknown>)
            : {};
        handler(values);
      });
    },
  };
}
