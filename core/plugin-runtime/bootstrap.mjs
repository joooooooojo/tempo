#!/usr/bin/env node
// Tempo plugin Runtime bootstrap.
//
// Started by the Rust Supervisor as: `node bootstrap.mjs`, cwd = the plugin's read-only
// install directory. The first stdin line is a JSON handshake descriptor (never argv/env):
//
//   { socketAddress, token, pluginId, mainPath, dataPath, runtimeVersion }
//
// Speaks `u32 BE length + UTF-8 JSON` framed protocol. Private UI↔Runtime uses Electron-style
// ipc (invoke/handle, send/on); Actions use commands.register and MCP Tools use mcpTools.register.

import net from "node:net";
import process from "node:process";
import { Buffer } from "node:buffer";
import { randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";
import {
  scaDecodeArgs,
  scaEncode,
  scaEncodeArgs,
  isScaEnvelope,
} from "./structured-clone.mjs";

const MAX_MESSAGE_BYTES = 1024 * 1024;
const COMMAND_TIMEOUT_MS = 30_000;
const COMMAND_GRACE_MS = 5_000;
const HANDSHAKE_TIMEOUT_MS = 5_000;

function log(level, message) {
  send({ type: "log", level, message: String(message) });
}

// -- Length-prefixed JSON framing -------------------------------------------------------

let socket;
let handshakePending;
let recvBuffer = Buffer.alloc(0);

function encodeFrame(value) {
  const body = Buffer.from(JSON.stringify(value), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length, 0);
  return Buffer.concat([header, body]);
}

function send(value) {
  if (!socket || socket.destroyed) return;
  try {
    socket.write(encodeFrame(value));
  } catch {
    // Host connection gone.
  }
}

function onSocketData(chunk) {
  recvBuffer = recvBuffer.length ? Buffer.concat([recvBuffer, chunk]) : chunk;
  for (;;) {
    if (recvBuffer.length < 4) return;
    const len = recvBuffer.readUInt32BE(0);
    if (len > MAX_MESSAGE_BYTES) {
      log("error", `frame exceeds ${MAX_MESSAGE_BYTES} bytes; closing connection`);
      socket.destroy();
      return;
    }
    if (recvBuffer.length < 4 + len) return;
    const body = recvBuffer.subarray(4, 4 + len);
    recvBuffer = recvBuffer.subarray(4 + len);
    let value;
    try {
      value = JSON.parse(body.toString("utf8"));
    } catch (error) {
      log("error", `failed to parse frame: ${error}`);
      continue;
    }
    handleHostFrame(value);
  }
}

// -- Host -> runtime requests (host.* responses) ----------------------------------------

const pendingHostRequests = new Map();

function callHost(method, params) {
  return new Promise((resolve, reject) => {
    const id = randomUUID();
    pendingHostRequests.set(id, { resolve, reject });
    send({ type: "request", id, method, params });
  });
}

// -- External Commands, MCP Tools, and Electron-style IPC --------------------------------

const commands = new Map();
const mcpTools = new Map();
const ipcHandlers = new Map();
const ipcSendListeners = new Map();
const activeInvocations = new Map();

function registerCommand(id, handler) {
  if (typeof id !== "string" || !id) {
    throw new TypeError("registerCommand requires a non-empty string id");
  }
  if (typeof handler !== "function") {
    throw new TypeError("registerCommand requires a handler function");
  }
  commands.set(id, handler);
}

function registerMcpTool(name, handler) {
  if (typeof name !== "string" || !name) {
    throw new TypeError("tempo.mcpTools.register requires a non-empty MCP tool name");
  }
  if (typeof handler !== "function") {
    throw new TypeError("tempo.mcpTools.register requires a handler function");
  }
  mcpTools.set(name, handler);
}

function ipcHandle(channel, handler) {
  if (typeof channel !== "string" || !channel) {
    throw new TypeError("ipcMain.handle requires a non-empty string channel");
  }
  if (typeof handler !== "function") {
    throw new TypeError("ipcMain.handle requires a handler function");
  }
  ipcHandlers.set(channel, handler);
}

function ipcOn(channel, listener) {
  if (typeof channel !== "string" || !channel) {
    throw new TypeError("ipcMain.on requires a non-empty string channel");
  }
  if (typeof listener !== "function") {
    throw new TypeError("ipcMain.on requires a listener function");
  }
  if (!ipcSendListeners.has(channel)) ipcSendListeners.set(channel, new Set());
  ipcSendListeners.get(channel).add(listener);
  return () => ipcSendListeners.get(channel)?.delete(listener);
}

function ipcSendToUi(channel, ...args) {
  let payload;
  try {
    payload = scaEncodeArgs(args);
  } catch (error) {
    log(
      "warn",
      `ipcMain.send failed to serialize args for ${channel}: ${error && error.message ? error.message : error}`,
    );
    return;
  }
  send({
    type: "event",
    event: String(channel),
    payload,
  });
}

async function runCommandHandler(message) {
  const { id, commandId, params } = message;
  const handler = commands.get(commandId);
  if (!handler) {
    send({
      type: "response",
      id,
      ok: false,
      error: { code: "NOT_FOUND", message: `unknown command: ${commandId}` },
    });
    return;
  }

  const controller = new AbortController();
  activeInvocations.set(id, controller);
  const timer = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS);
  const graceTimer = setTimeout(() => {
    log("warn", `command ${commandId} exceeded grace period after abort`);
  }, COMMAND_TIMEOUT_MS + COMMAND_GRACE_MS);

  try {
    const result = await handler(params, controller.signal);
    send({ type: "response", id, ok: true, result: result === undefined ? null : result });
  } catch (error) {
    if (controller.signal.aborted) {
      send({
        type: "response",
        id,
        ok: false,
        error: { code: "TIMEOUT", message: "command timed out" },
      });
    } else {
      send({
        type: "response",
        id,
        ok: false,
        error: {
          code: "COMMAND_FAILED",
          message: error && error.message ? String(error.message) : String(error),
          data: error && error.data !== undefined ? error.data : undefined,
        },
      });
    }
  } finally {
    clearTimeout(timer);
    clearTimeout(graceTimer);
    activeInvocations.delete(id);
  }
}

async function runMcpToolHandler(message) {
  const { id, toolName, arguments: input } = message;
  const handler = mcpTools.get(toolName);
  if (!handler) {
    send({
      type: "response",
      id,
      ok: false,
      error: { code: "NOT_FOUND", message: `unknown MCP tool: ${toolName}` },
    });
    return;
  }

  const controller = new AbortController();
  activeInvocations.set(id, controller);
  const timer = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS);
  const graceTimer = setTimeout(() => {
    log("warn", `MCP tool ${toolName} exceeded grace period after abort`);
  }, COMMAND_TIMEOUT_MS + COMMAND_GRACE_MS);

  try {
    const result = await handler(input, controller.signal);
    send({ type: "response", id, ok: true, result: result === undefined ? null : result });
  } catch (error) {
    if (controller.signal.aborted) {
      send({
        type: "response",
        id,
        ok: false,
        error: { code: "TIMEOUT", message: "MCP tool timed out" },
      });
    } else {
      send({
        type: "response",
        id,
        ok: false,
        error: {
          code: "MCP_TOOL_FAILED",
          message: error && error.message ? String(error.message) : String(error),
          data: error && error.data !== undefined ? error.data : undefined,
        },
      });
    }
  } finally {
    clearTimeout(timer);
    clearTimeout(graceTimer);
    activeInvocations.delete(id);
  }
}

async function handleIpcInvoke(message) {
  const { id, channel, args } = message;
  const handler = ipcHandlers.get(channel);
  if (!handler) {
    send({
      type: "response",
      id,
      ok: false,
      error: { code: "NOT_FOUND", message: `unknown ipc channel: ${channel}` },
    });
    return;
  }

  let decodedArgs;
  try {
    decodedArgs = isScaEnvelope(args) ? scaDecodeArgs(args) : Array.isArray(args) ? args : [args];
  } catch (error) {
    send({
      type: "response",
      id,
      ok: false,
      error: {
        code: "INVALID_REQUEST",
        message: error && error.message ? String(error.message) : "failed to deserialize ipc args",
      },
    });
    return;
  }

  const controller = new AbortController();
  activeInvocations.set(id, controller);
  const timer = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS);
  const graceTimer = setTimeout(() => {
    log("warn", `ipc ${channel} exceeded grace period after abort`);
  }, COMMAND_TIMEOUT_MS + COMMAND_GRACE_MS);

  try {
    const event = { sender: { send: ipcSendToUi } };
    const result = await handler(event, ...decodedArgs);
    let encodedResult;
    try {
      encodedResult = scaEncode(result === undefined ? null : result);
    } catch (error) {
      send({
        type: "response",
        id,
        ok: false,
        error: {
          code: "COMMAND_FAILED",
          message: error && error.message ? String(error.message) : "failed to serialize ipc result",
        },
      });
      return;
    }
    send({ type: "response", id, ok: true, result: encodedResult });
  } catch (error) {
    if (controller.signal.aborted) {
      send({
        type: "response",
        id,
        ok: false,
        error: { code: "TIMEOUT", message: "ipc timed out" },
      });
    } else {
      send({
        type: "response",
        id,
        ok: false,
        error: {
          code: "COMMAND_FAILED",
          message: error && error.message ? String(error.message) : String(error),
          data: error && error.data !== undefined ? error.data : undefined,
        },
      });
    }
  } finally {
    clearTimeout(timer);
    clearTimeout(graceTimer);
    activeInvocations.delete(id);
  }
}

function handleIpcSend(message) {
  const channel = String(message.channel ?? "");
  let args;
  try {
    args = isScaEnvelope(message.args)
      ? scaDecodeArgs(message.args)
      : Array.isArray(message.args)
        ? message.args
        : message.args === undefined
          ? []
          : [message.args];
  } catch (error) {
    log(
      "warn",
      `ipc-send failed to deserialize args for ${channel}: ${error && error.message ? error.message : error}`,
    );
    return;
  }
  const event = { sender: { send: ipcSendToUi } };
  for (const listener of ipcSendListeners.get(channel) ?? []) {
    try {
      listener(event, ...args);
    } catch (error) {
      log("warn", `ipcMain.on handler failed for ${channel}: ${error}`);
    }
  }
}

function handleCancel(message) {
  const controller = activeInvocations.get(message.id);
  if (controller) controller.abort();
}

async function handleShutdown() {
  try {
    await Promise.race([
      runUnmountedHooks(),
      new Promise((resolve) => setTimeout(resolve, COMMAND_GRACE_MS)),
    ]);
  } catch (error) {
    log("warn", String(error));
  } finally {
    process.exit(0);
  }
}

function handleHostFrame(message) {
  if (handshakePending) {
    if (message?.type !== "response" || message.id !== "handshake" || message.ok !== true) {
      handshakePending.reject(new Error("invalid host handshake acknowledgement"));
    } else {
      handshakePending.resolve();
    }
    return;
  }
  switch (message?.type) {
    case "response": {
      const pending = pendingHostRequests.get(message.id);
      if (!pending) return;
      pendingHostRequests.delete(message.id);
      if (message.ok) pending.resolve(message.result);
      else pending.reject(Object.assign(new Error(message.error?.message ?? "host call failed"), message.error));
      return;
    }
    case "invoke":
      void runCommandHandler(message);
      return;
    case "mcp-invoke":
      void runMcpToolHandler(message);
      return;
    case "ipc-invoke":
      void handleIpcInvoke(message);
      return;
    case "ipc-send":
      handleIpcSend(message);
      return;
    case "cancel":
      handleCancel(message);
      return;
    case "event":
      dispatchRuntimeEvent(message.event, message.payload);
      return;
    case "shutdown":
      void handleShutdown();
      return;
    default:
      return;
  }
}

// -- Host → runtime events --------------------------------------------------------------

const runtimeEventListeners = new Map();
const runtimeInternalEventListeners = new Map();
const ORIGINAL_EVENT_LISTENER = Symbol("tempo.originalEventListener");
const SPECIALIZED_RUNTIME_EVENTS = new Set(["settings.changed", "theme.changed"]);

function runtimeEventName(event) {
  if (typeof event !== "string" || !event.trim()) {
    throw new TypeError("tempo.events event must be a non-empty string");
  }
  return event.trim();
}

function assertRuntimeEventHandler(handler) {
  if (typeof handler !== "function") {
    throw new TypeError("tempo.events handler must be a function");
  }
}

function removeRuntimeEventListenerEntry(name, handler) {
  const listeners = runtimeEventListeners.get(name);
  if (!listeners) return false;
  const removed = listeners.delete(handler);
  if (listeners.size === 0) runtimeEventListeners.delete(name);
  return removed;
}

function offRuntimeEvent(event, handler) {
  const name = runtimeEventName(event);
  assertRuntimeEventHandler(handler);
  const listeners = runtimeEventListeners.get(name);
  if (!listeners) return false;
  let removed = false;
  for (const listener of listeners) {
    if (listener === handler || listener[ORIGINAL_EVENT_LISTENER] === handler) {
      listeners.delete(listener);
      removed = true;
    }
  }
  if (listeners.size === 0) runtimeEventListeners.delete(name);
  return removed;
}

function onRuntimeEvent(event, handler) {
  const name = runtimeEventName(event);
  assertRuntimeEventHandler(handler);
  if (!runtimeEventListeners.has(name)) runtimeEventListeners.set(name, new Set());
  runtimeEventListeners.get(name).add(handler);
  return () => {
    removeRuntimeEventListenerEntry(name, handler);
  };
}

function onInternalRuntimeEvent(event, handler) {
  const name = runtimeEventName(event);
  assertRuntimeEventHandler(handler);
  if (!runtimeInternalEventListeners.has(name)) {
    runtimeInternalEventListeners.set(name, new Set());
  }
  runtimeInternalEventListeners.get(name).add(handler);
  return () => {
    const listeners = runtimeInternalEventListeners.get(name);
    listeners?.delete(handler);
    if (listeners?.size === 0) runtimeInternalEventListeners.delete(name);
  };
}

function onceRuntimeEvent(event, handler) {
  const name = runtimeEventName(event);
  assertRuntimeEventHandler(handler);
  const wrapped = (payload) => {
    removeRuntimeEventListenerEntry(name, wrapped);
    handler(payload);
  };
  wrapped[ORIGINAL_EVENT_LISTENER] = handler;
  return onRuntimeEvent(name, wrapped);
}

function removeAllRuntimeEventListeners(event) {
  if (event === undefined) {
    runtimeEventListeners.clear();
    return;
  }
  runtimeEventListeners.delete(runtimeEventName(event));
}

function runtimeEventListenerCount(event) {
  return runtimeEventListeners.get(runtimeEventName(event))?.size ?? 0;
}

function dispatchRuntimeEvent(event, payload) {
  const name = String(event ?? "");
  const handlers = [
    ...(runtimeInternalEventListeners.get(name) ?? []),
    ...(SPECIALIZED_RUNTIME_EVENTS.has(name) ? [] : (runtimeEventListeners.get(name) ?? [])),
  ];
  for (const handler of handlers) {
    try {
      handler(payload ?? null);
    } catch (error) {
      log("warn", `runtime event handler failed for ${name}: ${error}`);
    }
  }
}

// -- Host-injected globals ---------------------------------------------------------------

function buildTempo(descriptor) {
  const storage = {
    get: async (key) => {
      const result = await callHost("storage.plugin.get", { key });
      return result?.value ?? null;
    },
    set: (key, value) => callHost("storage.plugin.set", { key, value }),
    delete: (key) => callHost("storage.plugin.delete", { key }),
    list: async () => {
      const result = await callHost("storage.plugin.list", {});
      return Array.isArray(result?.keys) ? result.keys : [];
    },
  };
  const files = {
    async readText(path) {
      const result = await callHost("files.readText", { path });
      return Buffer.from(String(result?.base64 ?? ""), "base64").toString("utf8");
    },
    writeText: (path, content) =>
      callHost("files.writeText", {
        path,
        base64: Buffer.from(String(content), "utf8").toString("base64"),
      }).then(() => undefined),
    async readBytes(path) {
      const result = await callHost("files.readBytes", { path });
      return new Uint8Array(Buffer.from(String(result?.base64 ?? ""), "base64"));
    },
    writeBytes(path, bytes) {
      if (!(bytes instanceof Uint8Array)) {
        throw new TypeError("tempo.files.writeBytes requires a Uint8Array");
      }
      return callHost("files.writeBytes", {
        path,
        base64: Buffer.from(bytes).toString("base64"),
      }).then(() => undefined);
    },
    async list(path = "") {
      const result = await callHost("files.list", { path });
      return Array.isArray(result?.entries) ? result.entries : [];
    },
    async stat(path) {
      const result = await callHost("files.stat", { path });
      return result?.stat ?? null;
    },
    mkdir: (path, options = {}) =>
      callHost("files.mkdir", {
        path,
        recursive: options.recursive === true,
      }).then(() => undefined),
    remove: (path, options = {}) =>
      callHost("files.remove", {
        path,
        recursive: options.recursive === true,
      }).then(() => undefined),
    rename: (from, to) =>
      callHost("files.rename", { from, to }).then(() => undefined),
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
      return onInternalRuntimeEvent("settings.changed", (payload) => {
        handler(payload?.values && typeof payload.values === "object" ? payload.values : {});
      });
    },
  };
  return {
    pluginId: descriptor.pluginId,
    commands: {
      register: registerCommand,
    },
    mcpTools: {
      register: registerMcpTool,
    },
    events: {
      on: onRuntimeEvent,
      once: onceRuntimeEvent,
      off: offRuntimeEvent,
      removeAllListeners: removeAllRuntimeEventListeners,
      listenerCount: runtimeEventListenerCount,
      eventNames: () => [...runtimeEventListeners.keys()],
    },
    storage,
    files,
    settings,
    notify: {
      show: (options = {}) => callHost("notify.show", options),
    },
    theme: {
      async get() {
        const result = await callHost("theme.get", {});
        return typeof result === "string" ? result : (result?.theme ?? "system");
      },
    },
    mainPanel: {
      hide: () => callHost("mainPanel.hide", {}).then(() => undefined),
    },
    app: {
      open: (appId, params) =>
        callHost("app.open", { appId, params: params ?? null }).then(() => undefined),
    },
    external: {
      open: (url) => callHost("external.open", { url }).then(() => undefined),
    },
    paths: {
      data: descriptor.dataPath,
    },
    runtime: {
      engine: "deno",
      version: descriptor.runtimeVersion ?? globalThis.Deno?.version.deno,
      nodeCompatVersion: process.versions.node,
    },
    host: callHost,
  };
}

// -- Boot sequence ------------------------------------------------------------------------

const mountedHooks = [];
const unmountedHooks = [];
let runtimeMounted = false;

async function runLifecycleHooks(hooks, name) {
  for (const hook of hooks.splice(0)) {
    try {
      await hook();
    } catch (error) {
      throw new Error(`${name} hook failed: ${error && error.message ? error.message : error}`);
    }
  }
}

async function runUnmountedHooks() {
  for (const hook of unmountedHooks.splice(0)) {
    try {
      await hook();
    } catch (error) {
      log(
        "warn",
        `onUnmounted hook failed: ${error && error.message ? error.message : error}`,
      );
    }
  }
}

function registerMountedHook(hook) {
  if (typeof hook !== "function") throw new TypeError("onMounted requires a function");
  if (runtimeMounted) queueMicrotask(() => void Promise.resolve(hook()));
  else mountedHooks.push(hook);
}

function registerUnmountedHook(hook) {
  if (typeof hook !== "function") throw new TypeError("onUnmounted requires a function");
  unmountedHooks.push(hook);
}

async function readHandshakeDescriptor() {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => onError(new Error("handshake descriptor timed out")), HANDSHAKE_TIMEOUT_MS);
    function cleanup() {
      clearTimeout(timer);
      process.stdin.off("data", onData);
      process.stdin.off("error", onError);
      process.stdin.off("end", onEnd);
    }
    function onData(chunk) {
      buffer += chunk.toString("utf8");
      if (Buffer.byteLength(buffer, "utf8") > MAX_MESSAGE_BYTES) {
        onError(new Error("handshake descriptor too large"));
        return;
      }
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex === -1) return;
      cleanup();
      const line = buffer.slice(0, newlineIndex);
      try {
        resolve(JSON.parse(line));
      } catch (error) {
        reject(error);
      }
    }
    function onError(error) {
      cleanup();
      reject(error);
    }
    function onEnd() {
      onError(new Error("stdin closed before handshake descriptor"));
    }
    process.stdin.on("data", onData);
    process.stdin.on("error", onError);
    process.stdin.on("end", onEnd);
  });
}

function connectionTarget(descriptor) {
  if (!descriptor || typeof descriptor !== "object") throw new Error("invalid handshake descriptor");
  for (const key of ["token", "pluginId", "mainPath", "dataPath"]) {
    if (typeof descriptor[key] !== "string" || !descriptor[key]) throw new Error(`invalid descriptor ${key}`);
  }
  const hasPath = descriptor.socketPath !== undefined;
  const hasAddress = descriptor.socketAddress !== undefined;
  if (hasPath === hasAddress) throw new Error("exactly one IPC endpoint is required");
  if (hasPath) {
    if (typeof descriptor.socketPath !== "string" || !descriptor.socketPath) throw new Error("invalid socketPath");
    return descriptor.socketPath;
  }
  const address = descriptor.socketAddress;
  if (!address || address.host !== "127.0.0.1" || !Number.isInteger(address.port) || address.port < 1 || address.port > 65535) {
    throw new Error("invalid loopback IPC address");
  }
  return { host: address.host, port: address.port };
}

async function main() {
  let descriptor;
  try {
    descriptor = await readHandshakeDescriptor();
  } catch (error) {
    console.error("failed to read handshake descriptor from stdin:", error);
    process.exit(1);
    return;
  }

  socket = net.createConnection(connectionTarget(descriptor));
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(new Error("IPC connection timed out")), HANDSHAKE_TIMEOUT_MS);
    function finish(error) {
      clearTimeout(timer);
      socket.off("connect", onConnect);
      socket.off("error", finish);
      if (error) reject(error);
      else resolve();
    }
    function onConnect() { finish(); }
    socket.once("connect", onConnect);
    socket.once("error", finish);
  });
  socket.on("data", onSocketData);
  socket.on("error", (error) => log("warn", `ipc socket error: ${error}`));
  socket.on("close", () => process.exit(runtimeMounted ? 0 : 1));

  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(new Error("host handshake timed out")), HANDSHAKE_TIMEOUT_MS);
    function finish(error) {
      clearTimeout(timer);
      handshakePending = undefined;
      if (error) reject(error);
      else resolve();
    }
    handshakePending = { resolve: () => finish(), reject: finish };
    send({ type: "handshake", token: descriptor.token });
  });

  globalThis.tempo = buildTempo(descriptor);
  globalThis.ipcMain = {
    handle: ipcHandle,
    on: ipcOn,
    send: ipcSendToUi,
  };
  globalThis.onMounted = registerMountedHook;
  globalThis.onUnmounted = registerUnmountedHook;

  try {
    await import(pathToFileURL(descriptor.mainPath).href);
    runtimeMounted = true;
    await runLifecycleHooks(mountedHooks, "onMounted");
    send({ type: "ready", ok: true });
  } catch (error) {
    send({
      type: "ready",
      ok: false,
      error: { code: "ACTIVATION_FAILED", message: error && error.message ? String(error.message) : String(error) },
    });
    setTimeout(() => process.exit(1), 100);
  }
}

process.on("uncaughtException", (error) => {
  log("error", `uncaught exception: ${error && error.stack ? error.stack : error}`);
});
process.on("unhandledRejection", (error) => {
  log("error", `unhandled rejection: ${error && error.stack ? error.stack : error}`);
});

main().catch((error) => {
  console.error("plugin runtime startup failed:", error?.message ?? String(error));
  process.exit(1);
});
