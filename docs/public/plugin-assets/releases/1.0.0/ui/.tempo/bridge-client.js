/** Host-injected globals for Tempo plugin UI pages. */
(() => {
  "use strict";

  if (window.tempo || window.ipcRenderer) return;

  const sca = globalThis.__tempoSca;
  const pending = new Map();
  const runtimeListeners = new Map();
  const platformListeners = new Map();
  const tempoEventListeners = new Map();
  const contextWaiters = [];
  const ORIGINAL_LISTENER = Symbol("tempo.originalListener");
  const SPECIALIZED_PLATFORM_EVENTS = new Set(["settings.changed", "theme.changed"]);
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
        "*",
      );
    });
  }

  function eventName(event) {
    if (typeof event !== "string" || !event.trim()) {
      throw new TypeError("event must be a non-empty string");
    }
    return event.trim();
  }

  function assertEventHandler(handler) {
    if (typeof handler !== "function") {
      throw new TypeError("event handler must be a function");
    }
  }

  function removeListenerEntry(map, name, handler) {
    const listeners = map.get(name);
    if (!listeners) return false;
    const removed = listeners.delete(handler);
    if (listeners.size === 0) map.delete(name);
    return removed;
  }

  function unlisten(map, event, handler) {
    const name = eventName(event);
    assertEventHandler(handler);
    const listeners = map.get(name);
    if (!listeners) return false;
    let removed = false;
    for (const listener of listeners) {
      if (listener === handler || listener[ORIGINAL_LISTENER] === handler) {
        listeners.delete(listener);
        removed = true;
      }
    }
    if (listeners.size === 0) map.delete(name);
    return removed;
  }

  function listen(map, event, handler) {
    const name = eventName(event);
    assertEventHandler(handler);
    if (!map.has(name)) map.set(name, new Set());
    map.get(name).add(handler);
    return () => {
      removeListenerEntry(map, name, handler);
    };
  }

  function listenOnce(map, event, handler) {
    const name = eventName(event);
    assertEventHandler(handler);
    const wrapped = (payload) => {
      removeListenerEntry(map, name, wrapped);
      handler(payload);
    };
    wrapped[ORIGINAL_LISTENER] = handler;
    return listen(map, name, wrapped);
  }

  function clearListeners(map, event) {
    if (event === undefined) {
      map.clear();
      return;
    }
    map.delete(eventName(event));
  }

  function countListeners(map, event) {
    return map.get(eventName(event))?.size ?? 0;
  }

  function emit(map, event, payload) {
    for (const handler of [...(map.get(event) ?? [])]) {
      try {
        handler(payload);
      } catch (error) {
        console.error(`[tempo] event handler failed for ${event}`, error);
      }
    }
  }

  function decodeResult(result) {
    return sca?.isScaEnvelope(result) ? sca.scaDecode(result) : result;
  }

  const ipcRenderer = {
    invoke(channel, ...args) {
      const name = String(channel ?? "").trim();
      if (!name) return Promise.reject(new Error("ipcRenderer channel is required"));
      if (!sca) return Promise.reject(new Error("structured clone codec unavailable"));
      try {
        return call(`ipc.invoke.${name}`, sca.scaEncodeArgs(args)).then(decodeResult);
      } catch (error) {
        return Promise.reject(error);
      }
    },
    send(channel, ...args) {
      const name = String(channel ?? "").trim();
      if (!name) throw new Error("ipcRenderer channel is required");
      if (!sca) throw new Error("structured clone codec unavailable");
      void call(`ipc.send.${name}`, sca.scaEncodeArgs(args)).catch((error) => {
        console.error(`[tempo] ipcRenderer.send failed for ${name}`, error);
      });
    },
    on(channel, listener) {
      const name = String(channel ?? "").trim();
      return listen(runtimeListeners, name, (payload) => {
        const event = { sender: "runtime" };
        try {
          if (sca?.isScaEnvelope(payload)) {
            listener(event, ...sca.scaDecodeArgs(payload));
          } else if (payload?.$ipcArgs && Array.isArray(payload.$ipcArgs)) {
            listener(event, ...payload.$ipcArgs);
          } else {
            listener(event, payload);
          }
        } catch (error) {
          console.error(`[tempo] ipcRenderer.on failed for ${name}`, error);
        }
      });
    },
  };

  const storage = {
    async get(key) {
      const result = await call("storage.plugin.get", { key });
      return result?.value ?? null;
    },
    async set(key, value) {
      await call("storage.plugin.set", { key, value });
    },
    async delete(key) {
      await call("storage.plugin.delete", { key });
    },
    async list() {
      const result = await call("storage.plugin.list", {});
      return Array.isArray(result?.keys) ? result.keys : [];
    },
  };

  const settings = {
    async getAll() {
      return { ...((await storage.get("__tempo/settings")) ?? {}) };
    },
    async get(id, fallback) {
      const values = await settings.getAll();
      return Object.prototype.hasOwnProperty.call(values, id) ? values[id] : fallback;
    },
    subscribe(handler) {
      return listen(platformListeners, "settings.changed", (payload) => {
        handler(payload?.values && typeof payload.values === "object" ? payload.values : {});
      });
    },
  };

  const tempo = {
    get context() {
      return context;
    },
    ready() {
      return context
        ? Promise.resolve(context)
        : new Promise((resolve) => contextWaiters.push(resolve));
    },
    events: {
      on(event, handler) {
        return listen(tempoEventListeners, event, handler);
      },
      once(event, handler) {
        return listenOnce(tempoEventListeners, event, handler);
      },
      off(event, handler) {
        return unlisten(tempoEventListeners, event, handler);
      },
      removeAllListeners(event) {
        clearListeners(tempoEventListeners, event);
      },
      listenerCount(event) {
        return countListeners(tempoEventListeners, event);
      },
      eventNames() {
        return [...tempoEventListeners.keys()];
      },
    },
    storage,
    settings,
    notify: {
      async show(options = {}) {
        await call("notify.show", options);
      },
    },
    theme: {
      async get() {
        const result = await call("theme.get", {});
        return typeof result === "string" ? result : (result?.theme ?? "system");
      },
      async subscribe(handler) {
        const result = await call("theme.onChange", {});
        const subscriptionId = result?.subscriptionId;
        if (!subscriptionId) throw new Error("theme subscription was not created");
        const off = listen(platformListeners, "theme.changed", (payload) => {
          handler(String(payload?.theme ?? "system"));
        });
        return () => {
          off();
          void call("subscription.release", { subscriptionId });
        };
      },
    },
    mainPanel: {
      hide: () => call("mainPanel.hide", {}).then(() => undefined),
      back: () => call("mainPanel.back", {}).then(() => undefined),
      setSize: (height) => call("mainPanel.setSize", { height }).then(() => undefined),
    },
    window: {
      setRect: (rect) => call("window.setRect", rect).then(() => undefined),
      close: () => call("window.close", {}).then(() => undefined),
    },
    app: {
      open: (appId, params) =>
        call("app.open", { appId, params: params ?? null }).then(() => undefined),
    },
    external: {
      open: (url) => call("external.open", { url }).then(() => undefined),
    },
    session: {
      push: (payload) => call("session.push", { payload }).then(() => undefined),
    },
    host(method, params) {
      const name = String(method ?? "").trim().replace(/^host\./, "");
      if (!HOST_METHODS.has(name) && !name.startsWith("storage.plugin.")) {
        return Promise.reject(new Error(`unknown host method: ${name}`));
      }
      return call(name, params);
    },
  };

  window.addEventListener("message", (event) => {
    if (event.source !== window.parent) return;
    const data = event.data;
    if (!data || typeof data !== "object") return;

    if (data.type === "tempo-plugin-rpc-response") {
      const entry = pending.get(data.id);
      if (!entry) return;
      pending.delete(data.id);
      if (data.ok) entry.resolve(data.result);
      else entry.reject(Object.assign(new Error(data.error?.message ?? "plugin call failed"), data.error ?? {}));
      return;
    }

    if (data.type === "tempo-plugin-context") {
      context = data;
      while (contextWaiters.length) contextWaiters.shift()(data);
      return;
    }

    if (data.type === "tempo-plugin-event") {
      if (data.source === "runtime") {
        emit(runtimeListeners, data.event, data.payload);
      } else {
        emit(platformListeners, data.event, data.payload);
        if (!SPECIALIZED_PLATFORM_EVENTS.has(data.event)) {
          emit(tempoEventListeners, data.event, data.payload);
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

  window.addEventListener(
    "keydown",
    (event) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      void tempo.mainPanel.back();
    },
    true,
  );

  window.tempo = tempo;
  window.ipcRenderer = ipcRenderer;
  window.parent.postMessage({ type: "tempo-plugin-ready" }, "*");
})();
