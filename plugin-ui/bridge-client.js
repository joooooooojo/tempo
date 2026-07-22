/**
 * Host-injected Tempo plugin UI bridge (`__tempo__/client.js`).
 * Auto-mounted on every plugin HTML page as `window.plugin` — no SDK required.
 *
 * Namespaces are separate so Runtime command ids never collide with host methods:
 *
 *   await window.plugin.invoke("hello", { who: "Tempo" })      // Runtime only
 *   await window.plugin.host("notify.show", { title: "Hi" })   // Host only
 *   window.plugin.on("greeted", (payload) => { … })
 *   const ctx = await window.plugin.ready()
 */
(() => {
  "use strict";

  if (window.plugin) return;

  const pending = new Map();
  const eventListeners = new Map();
  const contextWaiters = [];
  let requestSeq = 0;
  let context = null;

  const HOST_METHODS = new Set([
    "palette.hide",
    "palette.back",
    "palette.setSize",
    "theme.get",
    "notify.show",
    "session.push",
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

  /** Runtime command — always `runtime.<api>`, never a host method. */
  function invoke(api, params) {
    if (typeof api !== "string" || !api.trim()) {
      return Promise.reject(new Error("plugin.invoke(api, params): api must be a non-empty string"));
    }
    const name = api.trim();
    if (name.startsWith("runtime.")) {
      return call(name, params);
    }
    if (name.startsWith("host.") || HOST_METHODS.has(name)) {
      return Promise.reject(
        new Error(
          `plugin.invoke("${name}"): that name is a host API — use plugin.host("${name.replace(/^host\./, "")}", params)`
        )
      );
    }
    return call(`runtime.${name}`, params);
  }

  /** Host Bridge method — never routed to Runtime. */
  function host(api, params) {
    if (typeof api !== "string" || !api.trim()) {
      return Promise.reject(new Error("plugin.host(api, params): api must be a non-empty string"));
    }
    const name = api.trim().replace(/^host\./, "");
    if (name.startsWith("runtime.")) {
      return Promise.reject(
        new Error(`plugin.host("${api}"): Runtime commands go through plugin.invoke(...)`)
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
        void host("palette.back");
      }
    },
    true
  );

  window.plugin = {
    invoke,
    host,
    on,
    get context() {
      return context;
    },
    ready() {
      return context ? Promise.resolve(context) : new Promise((resolve) => contextWaiters.push(resolve));
    },
  };
})();
