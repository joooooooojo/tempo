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

// packages/plugin-sdk/dist/ipc/structured-clone.js
var TYPED_ARRAY_CTORS = {
  Int8Array,
  Uint8Array,
  Uint8ClampedArray,
  Int16Array,
  Uint16Array,
  Int32Array,
  Uint32Array,
  Float32Array,
  Float64Array,
  BigInt64Array,
  BigUint64Array
};
function bytesToBase64(bytes) {
  if (typeof Buffer !== "undefined") {
    return Buffer.from(bytes).toString("base64");
  }
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1)
    binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}
function base64ToBytes(b64) {
  if (typeof Buffer !== "undefined") {
    return new Uint8Array(Buffer.from(b64, "base64"));
  }
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1)
    out[i] = binary.charCodeAt(i);
  return out;
}
function utf8Encode(text) {
  if (typeof TextEncoder !== "undefined")
    return new TextEncoder().encode(text);
  return Uint8Array.from(Buffer.from(text, "utf8"));
}
function utf8Decode(bytes) {
  if (typeof TextDecoder !== "undefined")
    return new TextDecoder().decode(bytes);
  return Buffer.from(bytes).toString("utf8");
}
function isPlainObject(value) {
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}
function reject(value) {
  const kind = value === null ? "null" : typeof value === "object" ? value.constructor?.name ?? "Object" : typeof value;
  throw new Error(`Failed to serialize arguments: ${kind} is not structured-cloneable`);
}
function encodeValue(value, seen, out) {
  if (value === void 0)
    return { t: "u" };
  if (value === null)
    return null;
  const ty = typeof value;
  if (ty === "boolean" || ty === "string")
    return value;
  if (ty === "number") {
    if (Number.isNaN(value) || !Number.isFinite(value)) {
      return { t: "num", v: value };
    }
    return value;
  }
  if (ty === "bigint")
    return { t: "bi", v: value.toString() };
  if (ty === "symbol" || ty === "function")
    reject(value);
  if (ty !== "object")
    reject(value);
  const obj = value;
  if (seen.has(obj))
    return { t: "r", i: seen.get(obj) };
  if (typeof Promise !== "undefined" && value instanceof Promise)
    reject(value);
  if (typeof WeakMap !== "undefined" && value instanceof WeakMap)
    reject(value);
  if (typeof WeakSet !== "undefined" && value instanceof WeakSet)
    reject(value);
  if (typeof Element !== "undefined" && value instanceof Element)
    reject(value);
  if (typeof Node !== "undefined" && value instanceof Node)
    reject(value);
  if (value instanceof Date) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = { t: "date", v: value.getTime() };
    out.push(ir2);
    return { t: "r", i: i2 };
  }
  if (value instanceof ArrayBuffer) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = { t: "ab", v: bytesToBase64(new Uint8Array(value)) };
    out.push(ir2);
    return { t: "r", i: i2 };
  }
  if (ArrayBuffer.isView(value)) {
    const i2 = out.length;
    seen.set(obj, i2);
    const view = value;
    const bytes = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    if (typeof DataView !== "undefined" && value instanceof DataView) {
      const ir3 = {
        t: "dv",
        v: bytesToBase64(bytes),
        o: 0,
        l: view.byteLength
      };
      out.push(ir3);
      return { t: "r", i: i2 };
    }
    const name = value.constructor.name;
    if (!TYPED_ARRAY_CTORS[name])
      reject(value);
    const ir2 = { t: "ta", n: name, v: bytesToBase64(bytes) };
    out.push(ir2);
    return { t: "r", i: i2 };
  }
  if (value instanceof Map) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = { t: "m", v: [] };
    out.push(ir2);
    for (const [k, v] of value) {
      ir2.v.push([encodeValue(k, seen, out), encodeValue(v, seen, out)]);
    }
    return { t: "r", i: i2 };
  }
  if (value instanceof Set) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = { t: "s", v: [] };
    out.push(ir2);
    for (const item of value) {
      ir2.v.push(encodeValue(item, seen, out));
    }
    return { t: "r", i: i2 };
  }
  if (value instanceof Error) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = {
      t: "e",
      n: value.name || "Error",
      m: value.message || "",
      c: value.cause !== void 0 ? encodeValue(value.cause, seen, out) : void 0
    };
    out.push(ir2);
    return { t: "r", i: i2 };
  }
  if (Array.isArray(value)) {
    const i2 = out.length;
    seen.set(obj, i2);
    const ir2 = { t: "a", v: [] };
    out.push(ir2);
    for (const item of value) {
      ir2.v.push(encodeValue(item, seen, out));
    }
    return { t: "r", i: i2 };
  }
  if (!isPlainObject(obj))
    reject(value);
  const i = out.length;
  seen.set(obj, i);
  const ir = { t: "o", v: [] };
  out.push(ir);
  for (const key of Object.keys(value)) {
    ir.v.push([key, encodeValue(value[key], seen, out)]);
  }
  return { t: "r", i };
}
function decodeIr(ir, table) {
  if (ir === null || typeof ir === "boolean" || typeof ir === "number" || typeof ir === "string") {
    return ir;
  }
  if (typeof ir !== "object" || !("t" in ir))
    reject(ir);
  switch (ir.t) {
    case "u":
      return void 0;
    case "bi":
      return BigInt(ir.v);
    case "num":
      return Number(ir.v);
    case "r": {
      if (ir.i < 0 || ir.i >= table.length) {
        throw new Error("Failed to deserialize: invalid ref");
      }
      return table[ir.i];
    }
    default:
      throw new Error(`Failed to deserialize: unexpected tag ${String(ir.t)}`);
  }
}
function materializeSlot(slot, table, index) {
  if (slot === null || typeof slot !== "object" || !("t" in slot)) {
    table[index] = slot;
    return;
  }
  switch (slot.t) {
    case "date":
      table[index] = new Date(slot.v);
      return;
    case "ab":
      table[index] = base64ToBytes(slot.v).buffer;
      return;
    case "ta": {
      const Ctor = TYPED_ARRAY_CTORS[slot.n];
      if (!Ctor)
        throw new Error(`Failed to deserialize: unknown TypedArray ${slot.n}`);
      const bytes = base64ToBytes(slot.v);
      table[index] = new Ctor(bytes.buffer, bytes.byteOffset, bytes.byteLength / Ctor.BYTES_PER_ELEMENT);
      return;
    }
    case "dv": {
      const bytes = base64ToBytes(slot.v);
      table[index] = new DataView(bytes.buffer, bytes.byteOffset, slot.l);
      return;
    }
    case "a":
      table[index] = new Array(slot.v.length);
      return;
    case "o":
      table[index] = {};
      return;
    case "m":
      table[index] = /* @__PURE__ */ new Map();
      return;
    case "s":
      table[index] = /* @__PURE__ */ new Set();
      return;
    case "e": {
      const err = new Error(slot.m);
      err.name = slot.n;
      table[index] = err;
      return;
    }
    default:
      throw new Error(`Failed to deserialize: bad slot tag`);
  }
}
function fillSlot(slot, table, index) {
  if (slot === null || typeof slot !== "object" || !("t" in slot))
    return;
  switch (slot.t) {
    case "a": {
      const arr = table[index];
      for (let i = 0; i < slot.v.length; i += 1) {
        arr[i] = decodeIr(slot.v[i], table);
      }
      return;
    }
    case "o": {
      const obj = table[index];
      for (const [k, v] of slot.v) {
        obj[k] = decodeIr(v, table);
      }
      return;
    }
    case "m": {
      const map = table[index];
      for (const [k, v] of slot.v) {
        map.set(decodeIr(k, table), decodeIr(v, table));
      }
      return;
    }
    case "s": {
      const set = table[index];
      for (const item of slot.v) {
        set.add(decodeIr(item, table));
      }
      return;
    }
    case "e": {
      const err = table[index];
      if (slot.c !== void 0)
        err.cause = decodeIr(slot.c, table);
      return;
    }
    default:
      return;
  }
}
function scaEncode(value) {
  const seen = /* @__PURE__ */ new Map();
  const table = [];
  const root = encodeValue(value, seen, table);
  const payload = { root, table };
  const bytes = utf8Encode(JSON.stringify(payload));
  return { $sca: bytesToBase64(bytes) };
}
function scaDecode(envelope) {
  if (!envelope || typeof envelope.$sca !== "string") {
    throw new Error("Failed to deserialize: missing $sca envelope");
  }
  const payload = JSON.parse(utf8Decode(base64ToBytes(envelope.$sca)));
  const table = new Array(payload.table.length);
  for (let i = 0; i < payload.table.length; i += 1) {
    materializeSlot(payload.table[i], table, i);
  }
  for (let i = 0; i < payload.table.length; i += 1) {
    fillSlot(payload.table[i], table, i);
  }
  return decodeIr(payload.root, table);
}
function isScaEnvelope(value) {
  return !!value && typeof value === "object" && !Array.isArray(value) && typeof value.$sca === "string" && Object.keys(value).length === 1;
}
function scaEncodeArgs(args) {
  return scaEncode(args);
}
function scaDecodeArgs(envelope) {
  const value = scaDecode(envelope);
  return Array.isArray(value) ? value : [value];
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
  const ipcSend = (channel, ...args) => {
    if (raw.ipc?.send)
      raw.ipc.send(channel, ...args);
    else
      raw.ui.emit(channel, args.length <= 1 ? args[0] : args);
  };
  const ipc = {
    handle(channel, handler) {
      if (raw.ipc?.handle)
        raw.ipc.handle(channel, handler);
      else {
        throw new Error("@tempo/plugin-sdk: Runtime ipc.handle requires Host API >= 1.5.0 (bootstrap ctx.ipc)");
      }
    },
    on(channel, listener) {
      if (raw.ipc?.on)
        return raw.ipc.on(channel, listener);
      throw new Error("@tempo/plugin-sdk: Runtime ipc.on requires Host API >= 1.5.0 (bootstrap ctx.ipc)");
    },
    send: ipcSend
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
    ipc,
    storage,
    settings: createSettingsApi(storage, on),
    notify: createNotifyApi(call),
    theme: createThemeApi(call, on, { ui: false }),
    mainPanel: { hide: () => raw.host.mainPanel.hide() },
    app: createAppApi(call),
    external: createExternalApi(call),
    ui: {
      emit: (event, payload) => ipcSend(event, payload)
    },
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
function wrapIpc(raw) {
  return {
    invoke: (channel, ...args) => raw.ipc.invoke(channel, ...args),
    send: (channel, ...args) => raw.ipc.send(channel, ...args),
    on: (channel, listener) => raw.ipc.on(channel, listener)
  };
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
  return {
    ready: () => raw.ready(),
    get context() {
      return raw.context;
    },
    ipc: wrapIpc(raw),
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
    ipc: wrapIpc(raw),
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
  isScaEnvelope,
  scaDecode,
  scaDecodeArgs,
  scaEncode,
  scaEncodeArgs,
  toHostRpcError,
  wrapRuntimeContext
};
