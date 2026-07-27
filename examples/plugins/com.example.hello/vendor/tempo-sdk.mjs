// packages/plugin-sdk/dist/constants.js
var TEMPO_SETTINGS_KEY = "__tempo/settings";
var SETTINGS_CHANGED_EVENT = "settings.changed";
var THEME_CHANGED_EVENT = "theme.changed";

// packages/plugin-sdk/dist/errors.js
var PluginCommandError = class extends Error {
  data;
  constructor(message, data) {
    super(message);
    this.name = "PluginCommandError";
    this.data = data;
  }
};
var HostRpcError = class extends Error {
  code;
  data;
  constructor(error) {
    super(error.message || "host call failed");
    this.name = "HostRpcError";
    this.code = error.code || "INTERNAL";
    this.data = error.data;
  }
};
function toHostRpcError(error) {
  if (error instanceof HostRpcError)
    return error;
  if (error && typeof error === "object") {
    const record = error;
    if (typeof record.message === "string") {
      return new HostRpcError({
        code: typeof record.code === "string" ? record.code : "INTERNAL",
        message: record.message,
        data: record.data
      });
    }
  }
  return new HostRpcError({
    code: "INTERNAL",
    message: error instanceof Error ? error.message : String(error)
  });
}

// packages/plugin-sdk/dist/storage.js
function createStorageApi(adapter) {
  return {
    async get(key) {
      const value = await adapter.get(key);
      return value ?? null;
    },
    set(key, value) {
      return adapter.set(key, value);
    },
    delete(key) {
      return adapter.delete(key);
    },
    list() {
      return adapter.list();
    },
    async update(key, updater) {
      const current = await adapter.get(key);
      const next = await updater(current);
      await adapter.set(key, next);
      return next;
    }
  };
}

// packages/plugin-sdk/dist/settings.js
function createSettingsApi(storage, onEvent) {
  async function getAll() {
    const value = await storage.get(TEMPO_SETTINGS_KEY);
    return { ...value ?? {} };
  }
  async function get(id, defaultValue) {
    const all = await getAll();
    if (Object.prototype.hasOwnProperty.call(all, id)) {
      return all[id];
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
        const values = payload && typeof payload === "object" && payload !== null && "values" in payload && payload.values && typeof payload.values === "object" ? payload.values : {};
        handler(values);
      });
    }
  };
}

// packages/plugin-sdk/dist/host.js
function createNotifyApi(call) {
  return {
    show(options) {
      return call("notify.show", options ?? {}).then(() => void 0);
    }
  };
}
function createAppApi(call) {
  return {
    open(appId, params) {
      return call("app.open", { appId, params: params ?? null }).then(() => void 0);
    }
  };
}
function createExternalApi(call) {
  return {
    open(url) {
      return call("external.open", { url }).then(() => void 0);
    }
  };
}
function createMainPanelApi(call, options) {
  const ui = options?.ui ?? false;
  return {
    hide() {
      return call("mainPanel.hide", {}).then(() => void 0);
    },
    back() {
      if (!ui) {
        return Promise.reject(new Error("mainPanel.back is only available in plugin UI"));
      }
      return call("mainPanel.back", {}).then(() => void 0);
    },
    setSize(height) {
      if (!ui) {
        return Promise.reject(new Error("mainPanel.setSize is only available in plugin UI"));
      }
      return call("mainPanel.setSize", { height }).then(() => void 0);
    }
  };
}
function createWindowApi(call) {
  return {
    setRect(rect) {
      return call("window.setRect", rect).then(() => void 0);
    },
    close() {
      return call("window.close", {}).then(() => void 0);
    }
  };
}
function createSessionApi(call) {
  return {
    push(payload) {
      return call("session.push", { payload }).then(() => void 0);
    }
  };
}
function createThemeApi(call, onEvent, options) {
  const ui = options?.ui ?? false;
  return {
    async get() {
      const result = await call("theme.get", {});
      if (result && typeof result === "object" && "theme" in result) {
        return result.theme ?? "system";
      }
      return result ?? "system";
    },
    async subscribe(handler) {
      if (!ui) {
        throw new Error("theme.subscribe is only available in plugin UI");
      }
      const result = await call("theme.onChange", {});
      const subscriptionId = result?.subscriptionId;
      if (!subscriptionId) {
        throw new Error("theme.onChange did not return subscriptionId");
      }
      const off = onEvent("theme.changed", (payload) => {
        const theme = payload && typeof payload === "object" && payload !== null && "theme" in payload ? String(payload.theme) : "system";
        handler(theme);
      });
      return () => {
        off();
        void call("subscription.release", { subscriptionId });
      };
    }
  };
}

// packages/plugin-sdk/dist/runtime/index.js
function createEventBus() {
  const listeners = /* @__PURE__ */ new Map();
  return {
    on(event, handler) {
      if (!listeners.has(event))
        listeners.set(event, /* @__PURE__ */ new Set());
      listeners.get(event).add(handler);
      return () => listeners.get(event)?.delete(handler);
    },
    emit(event, payload) {
      for (const handler of listeners.get(event) ?? []) {
        try {
          handler(payload);
        } catch (error) {
          console.error("[@tempo/plugin-sdk] event handler failed", error);
        }
      }
    }
  };
}
function wrapRuntimeContext(raw) {
  const bus = createEventBus();
  const on = (event, handler) => {
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
    list: () => raw.host.storage.plugin.list()
  });
  const call = async (method, params) => {
    switch (method) {
      case "notify.show":
        return raw.host.notify.show(params ?? {});
      case "theme.get":
        return raw.host.theme.get();
      case "mainPanel.hide":
        return raw.host.mainPanel.hide();
      case "app.open": {
        const body = params;
        return raw.host.app.open(body.appId, body.params ?? void 0);
      }
      case "external.open":
        return raw.host.external.open(params.url);
      default:
        throw new Error(`Runtime host method not available: ${method}`);
    }
  };
  return {
    pluginId: raw.pluginId,
    paths: raw.paths,
    runtime: raw.runtime,
    raw,
    commands: {
      register(id, handler) {
        raw.registerCommand(id, handler);
      }
    },
    storage,
    settings: createSettingsApi(storage, on),
    notify: createNotifyApi(call),
    theme: createThemeApi(call, on, { ui: false }),
    mainPanel: { hide: () => raw.host.mainPanel.hide() },
    app: createAppApi(call),
    external: createExternalApi(call),
    ui: raw.ui,
    on
  };
}
function definePlugin(definition) {
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
    deactivate: definition.deactivate
  };
}

// packages/plugin-sdk/dist/ui/index.js
function getBridge(explicit) {
  const bridge = explicit ?? (typeof window !== "undefined" ? window.plugin : void 0);
  if (!bridge) {
    throw new Error("@tempo/plugin-sdk: window.plugin is missing. Tempo injects it before your page runs.");
  }
  return bridge;
}
async function createPluginClient(options) {
  const raw = getBridge(options?.bridge);
  if (options?.ready !== false) {
    await raw.ready();
  }
  const call = async (method, params) => {
    try {
      return await raw.host(method, params ?? {});
    } catch (error) {
      throw toHostRpcError(error);
    }
  };
  const storage = createStorageApi({
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
    }
  });
  const client = {
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
    host: (method, params) => call(method, params),
    raw
  };
  return client;
}
function createPluginClientSync(bridge) {
  const raw = getBridge(bridge);
  const call = async (method, params) => {
    try {
      return await raw.host(method, params ?? {});
    } catch (error) {
      throw toHostRpcError(error);
    }
  };
  const storage = createStorageApi({
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
    }
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
    host: (method, params) => call(method, params),
    raw
  };
}
export {
  HostRpcError,
  PluginCommandError,
  SETTINGS_CHANGED_EVENT,
  TEMPO_SETTINGS_KEY,
  THEME_CHANGED_EVENT,
  createAppApi,
  createExternalApi,
  createMainPanelApi,
  createNotifyApi,
  createPluginClient,
  createPluginClientSync,
  createSessionApi,
  createSettingsApi,
  createStorageApi,
  createThemeApi,
  createWindowApi,
  definePlugin,
  toHostRpcError,
  wrapRuntimeContext
};
