import { createAppApi, createExternalApi, createNotifyApi, createThemeApi } from "../host.js";
import { createSettingsApi, type PluginSettingsApi } from "../settings.js";
import { createStorageApi, type PluginStorageApi } from "../storage.js";
import type { CommandHandler, Unsubscribe, IpcEvent, IpcListener } from "../types.js";
import type {
  AppApi,
  ExternalApi,
  MainPanelApi,
  NotifyApi,
  ThemeApi,
} from "../host.js";

export type { IpcEvent, IpcListener };

export type IpcInvokeHandler = (
  event: IpcEvent,
  ...args: unknown[]
) => unknown | Promise<unknown>;

/** Raw ExtensionContext injected by Tempo's Runtime bootstrap. */
export interface RawExtensionContext {
  pluginId: string;
  registerCommand<TParams = unknown, TResult = unknown>(
    id: string,
    handler: CommandHandler<TParams, TResult>
  ): void;
  host: {
    mainPanel: { hide(): Promise<void> };
    app: { open(appId: string, params?: Record<string, unknown>): Promise<void> };
    external: { open(url: string): Promise<void> };
    notify: { show(options: { title?: string; body?: string }): Promise<void> };
    theme: { get(): Promise<{ theme: string } | string> };
    storage: {
      plugin: {
        get<T = unknown>(key: string): Promise<T | null>;
        set(key: string, value: unknown): Promise<void>;
        delete(key: string): Promise<void>;
        list(): Promise<string[]>;
      };
    };
  };
  ipc?: {
    handle(channel: string, handler: IpcInvokeHandler): void;
    on(channel: string, listener: IpcListener): Unsubscribe;
    send(channel: string, ...args: unknown[]): void;
  };
  ui: { emit(event: string, payload?: unknown): void };
  paths: { data: string };
  runtime: { nodeVersion: string };
  on?(event: string, handler: (payload: unknown) => void): Unsubscribe;
}

export interface RuntimeCommandsApi {
  register<TParams = unknown, TResult = unknown>(
    id: string,
    handler: CommandHandler<TParams, TResult>
  ): void;
}

export interface RuntimeIpcApi {
  handle(channel: string, handler: IpcInvokeHandler): void;
  on(channel: string, listener: IpcListener): Unsubscribe;
  send(channel: string, ...args: unknown[]): void;
}

export interface RuntimeTempo {
  readonly pluginId: string;
  readonly paths: { data: string };
  readonly runtime: { nodeVersion: string };
  readonly commands: RuntimeCommandsApi;
  readonly ipc: RuntimeIpcApi;
  readonly storage: PluginStorageApi;
  readonly settings: PluginSettingsApi;
  readonly notify: NotifyApi;
  readonly theme: ThemeApi;
  readonly mainPanel: Pick<MainPanelApi, "hide">;
  readonly app: AppApi;
  readonly external: ExternalApi;
  /** @deprecated Prefer `tempo.ipc.send` */
  readonly ui: { emit(event: string, payload?: unknown): void };
  /** Low-level host event subscription (e.g. `settings.changed`). */
  on(event: string, handler: (payload: unknown) => void): Unsubscribe;
  readonly raw: RawExtensionContext;
}

export interface PluginDefinition {
  activate?(tempo: RuntimeTempo): void | Promise<void>;
  deactivate?(): void | Promise<void>;
  commands?: Record<string, CommandHandler>;
}

export interface PluginModule {
  activate(ctx: RawExtensionContext): void | Promise<void>;
  deactivate?(): void | Promise<void>;
}

function createEventBus() {
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  return {
    on(event: string, handler: (payload: unknown) => void): Unsubscribe {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event)!.add(handler);
      return () => listeners.get(event)?.delete(handler);
    },
    emit(event: string, payload: unknown) {
      for (const handler of listeners.get(event) ?? []) {
        try {
          handler(payload);
        } catch (error) {
          console.error("[@tempo/plugin-sdk] event handler failed", error);
        }
      }
    },
  };
}

export function wrapRuntimeContext(raw: RawExtensionContext): RuntimeTempo {
  const bus = createEventBus();
  const on: typeof bus.on = (event, handler) => {
    const localOff = bus.on(event, handler);
    const hostOff = raw.on?.(event, handler);
    return () => {
      localOff();
      hostOff?.();
    };
  };

  const storage = createStorageApi({
    get: (key) => raw.host.storage.plugin.get(key),
    set: (key, value) => raw.host.storage.plugin.set(key, value),
    delete: (key) => raw.host.storage.plugin.delete(key),
    list: () => raw.host.storage.plugin.list(),
  });

  const call = async (method: string, params?: unknown) => {
    switch (method) {
      case "notify.show":
        return raw.host.notify.show((params as { title?: string; body?: string }) ?? {});
      case "theme.get":
        return raw.host.theme.get();
      case "mainPanel.hide":
        return raw.host.mainPanel.hide();
      case "app.open": {
        const body = params as { appId: string; params?: Record<string, unknown> | null };
        return raw.host.app.open(body.appId, body.params ?? undefined);
      }
      case "external.open":
        return raw.host.external.open((params as { url: string }).url);
      default:
        throw new Error(`Runtime host method not available: ${method}`);
    }
  };

  const ipcSend = (channel: string, ...args: unknown[]) => {
    if (raw.ipc?.send) raw.ipc.send(channel, ...args);
    else raw.ui.emit(channel, args.length <= 1 ? args[0] : args);
  };

  const ipc: RuntimeIpcApi = {
    handle(channel, handler) {
      if (raw.ipc?.handle) raw.ipc.handle(channel, handler);
      else {
        throw new Error(
          "@tempo/plugin-sdk: Runtime ipc.handle requires Host API >= 1.5.0 (bootstrap ctx.ipc)",
        );
      }
    },
    on(channel, listener) {
      if (raw.ipc?.on) return raw.ipc.on(channel, listener);
      throw new Error(
        "@tempo/plugin-sdk: Runtime ipc.on requires Host API >= 1.5.0 (bootstrap ctx.ipc)",
      );
    },
    send: ipcSend,
  };

  return {
    pluginId: raw.pluginId,
    paths: raw.paths,
    runtime: raw.runtime,
    raw,
    commands: {
      register(id, handler) {
        raw.registerCommand(id, handler);
      },
    },
    ipc,
    storage,
    settings: createSettingsApi(storage, on),
    notify: createNotifyApi(call),
    theme: createThemeApi(call, on, { ui: false }),
    mainPanel: { hide: () => raw.host.mainPanel.hide() },
    app: createAppApi(call),
    external: createExternalApi(call),
    ui: {
      emit: (event, payload) => ipcSend(event, payload),
    },
    on,
  };
}

/**
 * Define a Runtime plugin module. Compatible with Tempo bootstrap:
 * `export const { activate, deactivate } = definePlugin({ ... })`
 * or `export default definePlugin({ ... })`.
 */
export function definePlugin(definition: PluginDefinition): PluginModule {
  return {
    async activate(raw) {
      const tempo = wrapRuntimeContext(raw);
      if (definition.commands) {
        for (const [id, handler] of Object.entries(definition.commands)) {
          tempo.commands.register(id, handler);
        }
      }
      await definition.activate?.(tempo);
    },
    deactivate: definition.deactivate,
  };
}
