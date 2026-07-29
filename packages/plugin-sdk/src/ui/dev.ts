import type { PluginUiContext, Unsubscribe } from "../types.js";
import {
  isScaEnvelope,
  scaDecode,
  scaDecodeArgs,
  scaEncodeArgs,
} from "../ipc/structured-clone.js";
import type { IpcListener, TempoPluginUiBridge, UiIpcApi } from "./index.js";

type PendingCall = {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
};

type HostMessage =
  | ({ type: "tempo-plugin-context" } & PluginUiContext)
  | {
      type: "tempo-plugin-rpc-response";
      id: string;
      ok: boolean;
      result?: unknown;
      error?: { message?: string };
    }
  | { type: "tempo-plugin-event"; event: string; payload: unknown }
  | { type: "tempo-plugin-dev-log"; source: string; message: string };

const HOST_METHODS = new Set([
  "mainPanel.hide",
  "mainPanel.back",
  "mainPanel.setSize",
  "window.setRect",
  "window.close",
  "theme.get",
  "theme.onChange",
  "notify.show",
  "session.push",
  "subscription.release",
  "storage.plugin.get",
  "storage.plugin.set",
  "storage.plugin.delete",
  "storage.plugin.list",
  "app.open",
  "external.open",
]);

/** Install Tempo's standard iframe Bridge when a UI is served by an external dev server. */
export function installPluginDevBridge(): TempoPluginUiBridge | undefined {
  if (typeof window === "undefined" || window.parent === window)
    return undefined;
  if (window.plugin) {
    window.parent.postMessage({ type: "tempo-plugin-ready" }, "*");
    return window.plugin;
  }

  const pending = new Map<string, PendingCall>();
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const contextWaiters: Array<(context: PluginUiContext) => void> = [];
  let context: PluginUiContext | null = null;
  let sequence = 0;

  const call = (method: string, params?: unknown): Promise<unknown> =>
    new Promise((resolve, reject) => {
      const id = `plugin-dev-${++sequence}-${Date.now()}`;
      pending.set(id, { resolve, reject });
      window.parent.postMessage(
        { type: "tempo-plugin-rpc", id, method, params: params ?? {} },
        "*",
      );
    });

  window.addEventListener("message", (event: MessageEvent<HostMessage>) => {
    if (event.source !== window.parent) return;
    const data = event.data;
    if (!data || typeof data !== "object") return;

    if (data.type === "tempo-plugin-context") {
      context = data;
      while (contextWaiters.length) contextWaiters.shift()?.(data);
      return;
    }
    if (data.type === "tempo-plugin-rpc-response") {
      const entry = pending.get(data.id);
      if (!entry) return;
      pending.delete(data.id);
      if (data.ok) entry.resolve(data.result);
      else entry.reject(new Error(data.error?.message ?? "plugin call failed"));
      return;
    }
    if (data.type === "tempo-plugin-event") {
      for (const handler of listeners.get(data.event) ?? [])
        handler(data.payload);
      return;
    }
    if (data.type === "tempo-plugin-dev-log") {
      const label = `[runtime:${data.source}]`;
      if (data.source === "stderr") console.error(label, data.message);
      else console.log(label, data.message);
    }
  });

  const decodeResult = (result: unknown) =>
    isScaEnvelope(result) ? scaDecode(result) : result;

  const onRaw = (event: string, handler: (payload: unknown) => void): Unsubscribe => {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event)?.add(handler);
    return () => listeners.get(event)?.delete(handler);
  };

  const ipc: UiIpcApi = {
    invoke(channel, ...args) {
      const name = String(channel ?? "").trim();
      if (!name) return Promise.reject(new Error("ipc channel is required"));
      try {
        const envelope = scaEncodeArgs(args);
        return call(`ipc.invoke.${name}`, envelope).then(decodeResult);
      } catch (error) {
        return Promise.reject(error);
      }
    },
    send(channel, ...args) {
      const name = String(channel ?? "").trim();
      if (!name) throw new Error("ipc channel is required");
      const envelope = scaEncodeArgs(args);
      void call(`ipc.send.${name}`, envelope);
    },
    on(channel, listener: IpcListener) {
      const name = String(channel ?? "").trim();
      if (!name) throw new Error("ipc channel is required");
      return onRaw(name, (payload) => {
        const event = { sender: "runtime" };
        try {
          if (isScaEnvelope(payload)) {
            listener(event, ...scaDecodeArgs(payload));
          } else if (
            payload &&
            typeof payload === "object" &&
            Array.isArray((payload as { $ipcArgs?: unknown }).$ipcArgs)
          ) {
            listener(event, ...((payload as { $ipcArgs: unknown[] }).$ipcArgs));
          } else {
            listener(event, payload);
          }
        } catch (error) {
          console.error("[plugin-dev] ipc.on failed to deserialize", error);
        }
      });
    },
  };

  const bridge: TempoPluginUiBridge = {
    ready: () =>
      context
        ? Promise.resolve(context)
        : new Promise((resolve) => contextWaiters.push(resolve)),
    get context() {
      return context;
    },
    ipc,
    host<TResult = unknown>(
      method: string,
      params?: unknown,
    ): Promise<TResult> {
      const name = method.trim().replace(/^host\./, "");
      if (!HOST_METHODS.has(name) && !name.startsWith("storage.plugin.")) {
        return Promise.reject(new Error(`unknown host method: ${name}`));
      }
      return call(name, params) as Promise<TResult>;
    },
    on: onRaw,
  };
  window.plugin = bridge;
  window.parent.postMessage({ type: "tempo-plugin-ready" }, "*");
  return bridge;
}

installPluginDevBridge();
