import {
  createAppApi,
  createExternalApi,
  createMainPanelApi,
  createNotifyApi,
  createSessionApi,
  createThemeApi,
  createWindowApi,
  type AppApi,
  type ExternalApi,
  type MainPanelApi,
  type NotifyApi,
  type SessionApi,
  type ThemeApi,
  type WindowApi,
} from "../host.js";
import { toHostRpcError } from "../errors.js";
import { createSettingsApi, type PluginSettingsApi } from "../settings.js";
import { createStorageApi, type PluginStorageApi } from "../storage.js";
import type { PluginUiContext, Unsubscribe } from "../types.js";

export interface TempoPluginUiBridge {
  ready(): Promise<PluginUiContext>;
  readonly context: PluginUiContext | null;
  invoke<TResult = unknown>(command: string, params?: unknown): Promise<TResult>;
  host<TResult = unknown>(method: string, params?: unknown): Promise<TResult>;
  on(event: string, handler: (payload: unknown) => void): Unsubscribe;
}

export interface UiTempo {
  ready(): Promise<PluginUiContext>;
  readonly context: PluginUiContext | null;
  invoke<TResult = unknown>(command: string, params?: unknown): Promise<TResult>;
  on(event: string, handler: (payload: unknown) => void): Unsubscribe;
  readonly storage: PluginStorageApi;
  readonly settings: PluginSettingsApi;
  readonly notify: NotifyApi;
  readonly theme: ThemeApi;
  readonly mainPanel: MainPanelApi;
  readonly window: WindowApi;
  readonly app: AppApi;
  readonly external: ExternalApi;
  readonly session: SessionApi;
  /** Escape hatch for rare host methods. */
  host<TResult = unknown>(method: string, params?: unknown): Promise<TResult>;
  readonly raw: TempoPluginUiBridge;
}

declare global {
  interface Window {
    plugin?: TempoPluginUiBridge;
  }
}

function getBridge(explicit?: TempoPluginUiBridge): TempoPluginUiBridge {
  const bridge = explicit ?? (typeof window !== "undefined" ? window.plugin : undefined);
  if (!bridge) {
    throw new Error(
      "@tempo/plugin-sdk: window.plugin is missing. Tempo injects it before your page runs."
    );
  }
  return bridge;
}

/**
 * Create the ergonomic UI client around the host-injected `window.plugin` bridge.
 *
 * ```ts
 * import { createPluginClient } from "@tempo/plugin-sdk";
 * const tempo = await createPluginClient();
 * const loud = await tempo.settings.get("loud", false);
 * await tempo.notify.show({ title: "Hi", body: "Ready" });
 * ```
 */
export async function createPluginClient(
  options?: { bridge?: TempoPluginUiBridge; ready?: boolean }
): Promise<UiTempo> {
  const raw = getBridge(options?.bridge);
  if (options?.ready !== false) {
    await raw.ready();
  }

  const call = async (method: string, params?: unknown) => {
    try {
      return await raw.host(method, params ?? {});
    } catch (error) {
      throw toHostRpcError(error);
    }
  };

  const storage = createStorageApi({
    async get(key) {
      const result = (await call("storage.plugin.get", { key })) as { value?: unknown };
      return result?.value ?? null;
    },
    async set(key, value) {
      await call("storage.plugin.set", { key, value });
    },
    async delete(key) {
      await call("storage.plugin.delete", { key });
    },
    async list() {
      const result = (await call("storage.plugin.list", {})) as { keys?: string[] };
      return Array.isArray(result?.keys) ? result.keys : [];
    },
  });

  const client: UiTempo = {
    ready: () => raw.ready(),
    get context() {
      return raw.context;
    },
    invoke: (command, params) => raw.invoke(command, params),
    on: (event, handler) => raw.on(event, handler),
    storage,
    settings: createSettingsApi(storage, (event, handler) => raw.on(event, handler)),
    notify: createNotifyApi(call),
    theme: createThemeApi(call, (event, handler) => raw.on(event, handler), { ui: true }),
    mainPanel: createMainPanelApi(call, { ui: true }),
    window: createWindowApi(call),
    app: createAppApi(call),
    external: createExternalApi(call),
    session: createSessionApi(call),
    host: <TResult = unknown>(method: string, params?: unknown) =>
      call(method, params) as Promise<TResult>,
    raw,
  };

  return client;
}

/** Synchronous wrapper when you already awaited `window.plugin.ready()`. */
export function createPluginClientSync(bridge?: TempoPluginUiBridge): UiTempo {
  const raw = getBridge(bridge);
  const call = async (method: string, params?: unknown) => {
    try {
      return await raw.host(method, params ?? {});
    } catch (error) {
      throw toHostRpcError(error);
    }
  };
  const storage = createStorageApi({
    async get(key) {
      const result = (await call("storage.plugin.get", { key })) as { value?: unknown };
      return result?.value ?? null;
    },
    async set(key, value) {
      await call("storage.plugin.set", { key, value });
    },
    async delete(key) {
      await call("storage.plugin.delete", { key });
    },
    async list() {
      const result = (await call("storage.plugin.list", {})) as { keys?: string[] };
      return Array.isArray(result?.keys) ? result.keys : [];
    },
  });

  return {
    ready: () => raw.ready(),
    get context() {
      return raw.context;
    },
    invoke: (command, params) => raw.invoke(command, params),
    on: (event, handler) => raw.on(event, handler),
    storage,
    settings: createSettingsApi(storage, (event, handler) => raw.on(event, handler)),
    notify: createNotifyApi(call),
    theme: createThemeApi(call, (event, handler) => raw.on(event, handler), { ui: true }),
    mainPanel: createMainPanelApi(call, { ui: true }),
    window: createWindowApi(call),
    app: createAppApi(call),
    external: createExternalApi(call),
    session: createSessionApi(call),
    host: <TResult = unknown>(method: string, params?: unknown) =>
      call(method, params) as Promise<TResult>,
    raw,
  };
}
