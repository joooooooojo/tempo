/**
 * Host-injected Tempo plugin UI bridge (`__tempo__/client.js`).
 * Auto-mounted on every plugin HTML page as `window.plugin` — no SDK required.
 *
 * Electron-style private UI ↔ Runtime IPC (Host API >= 1.5.0):
 *
 *   await window.plugin.ipc.invoke("greet", { who: "Tempo" })
 *   window.plugin.ipc.send("ping", { n: 1 })
 *   window.plugin.ipc.on("greeted", (event, payload) => { … })
 *
 * Host APIs only:
 *   await window.plugin.host("notify.show", { title: "Hi" })
 *   const ctx = await window.plugin.ready()
 *
 * UI cannot invoke Runtime commands (Action / Hook / MCP only).
 */
(() => {
  "use strict";

  if (window.plugin) return;

  const sca = globalThis.__tempoSca;
  if (!sca) {
    console.error("[plugin] structured-clone helper missing; inject __tempo__/structured-clone.js first");
  }

  const pending = new Map();
  const eventListeners = new Map();
  const contextWaiters = [];
  let requestSeq = 0;
  let context = null;

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

  function nextId() {
    requestSeq += 1;
    return `plugin-${requestSeq}-${Date.now()}`;
  }

  function call(method, params) {
    return new Promise((resolve, reject) => {
      const id = nextId();
      pending.set(id, { resolve, reject });
      window.parent.postMessage(
        { type: "tempo-plugin-rpc", id, method, params: params ?? {} },
        "*"
      );
    });
  }

  function decodeResult(result) {
    if (sca && sca.isScaEnvelope(result)) {
      return sca.scaDecode(result);
    }
    return result;
  }

  /** Electron-style ipcRenderer.invoke → Runtime ipc.handle (SCA args/result). */
  function ipcInvoke(channel, ...args) {
    if (typeof channel !== "string" || !channel.trim()) {
      return Promise.reject(new Error("plugin.ipc.invoke(channel, ...args): channel must be a non-empty string"));
    }
    const name = channel.trim();
    if (name.startsWith("ipc.") || name.startsWith("runtime.") || name.startsWith("host.") || HOST_METHODS.has(name)) {
      return Promise.reject(new Error(`plugin.ipc.invoke("${name}"): invalid channel name`));
    }
    let envelope;
    try {
      envelope = sca ? sca.scaEncodeArgs(args) : { $sca: "" };
      if (!sca) throw new Error("structured clone codec unavailable");
    } catch (error) {
      return Promise.reject(error);
    }
    return call(`ipc.invoke.${name}`, envelope).then(decodeResult);
  }

  /** Electron-style ipcRenderer.send → Runtime ipc.on (SCA args, fire-and-forget). */
  function ipcSend(channel, ...args) {
    if (typeof channel !== "string" || !channel.trim()) {
      throw new Error("plugin.ipc.send(channel, ...args): channel must be a non-empty string");
    }
    const name = channel.trim();
    if (!sca) throw new Error("structured clone codec unavailable");
    const envelope = sca.scaEncodeArgs(args);
    void call(`ipc.send.${name}`, envelope);
  }

  /** Receive Runtime ipc.send (SCA) and unwrap into Electron-style listener(event, ...args). */
  function ipcOn(channel, listener) {
    if (typeof channel !== "string" || !channel.trim()) {
      throw new Error("plugin.ipc.on(channel, listener): channel must be a non-empty string");
    }
    if (typeof listener !== "function") {
      throw new Error("plugin.ipc.on(channel, listener): listener must be a function");
    }
    return on(channel.trim(), (payload) => {
      const event = { sender: "runtime" };
      try {
        if (sca && sca.isScaEnvelope(payload)) {
          listener(event, ...sca.scaDecodeArgs(payload));
        } else if (payload && typeof payload === "object" && Array.isArray(payload.$ipcArgs)) {
          listener(event, ...payload.$ipcArgs);
        } else {
          listener(event, payload);
        }
      } catch (error) {
        console.error("[plugin] ipc.on failed to deserialize", error);
      }
    });
  }

  /** Host Bridge method — never routed to Runtime. */
  function host(api, params) {
    if (typeof api !== "string" || !api.trim()) {
      return Promise.reject(new Error("plugin.host(api, params): api must be a non-empty string"));
    }
    const name = api.trim().replace(/^host\./, "");
    if (
      name.startsWith("runtime.") ||
      name.startsWith("rpc.") ||
      name.startsWith("ipc.")
    ) {
      return Promise.reject(
        new Error(`plugin.host("${api}"): use plugin.ipc for UI↔Runtime; host methods only here`)
      );
    }
    if (!HOST_METHODS.has(name) && !name.startsWith("storage.plugin.")) {
      return Promise.reject(new Error(`plugin.host("${name}"): unknown host method`));
    }
    return call(name, params);
  }

  window.addEventListener("message", (event) => {
    if (event.source !== window.parent) return;
    const data = event.data;
    if (!data || typeof data !== "object") return;

    if (data.type === "tempo-plugin-rpc-response") {
      const entry = pending.get(data.id);
      if (!entry) return;
      pending.delete(data.id);
      if (data.ok) entry.resolve(data.result);
      else {
        entry.reject(
          Object.assign(new Error(data.error?.message ?? "plugin call failed"), data.error ?? {})
        );
      }
      return;
    }

    if (data.type === "tempo-plugin-context") {
      context = data;
      while (contextWaiters.length) contextWaiters.shift()(data);
      return;
    }

    if (data.type === "tempo-plugin-event") {
      for (const handler of eventListeners.get(data.event) ?? []) {
        try {
          handler(data.payload);
        } catch (error) {
          console.error("[plugin] event handler failed", error);
        }
      }
      return;
    }

    if (data.type === "tempo-plugin-dev-log") {
      const label = `[runtime:${data.source}]`;
      if (data.source === "stderr") console.error(label, data.message);
      else console.log(label, data.message);
    }
  });

  function on(event, handler) {
    if (!eventListeners.has(event)) eventListeners.set(event, new Set());
    eventListeners.get(event).add(handler);
    return () => eventListeners.get(event)?.delete(handler);
  }

  window.addEventListener(
    "keydown",
    (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void host("mainPanel.back");
      }
    },
    true
  );

  window.plugin = {
    ipc: {
      invoke: ipcInvoke,
      send: ipcSend,
      on: ipcOn,
    },
    host,
    on,
    get context() {
      return context;
    },
    ready() {
      return context ? Promise.resolve(context) : new Promise((resolve) => contextWaiters.push(resolve));
    },
  };

  window.parent.postMessage({ type: "tempo-plugin-ready" }, "*");
})();
