#!/usr/bin/env node
// Tempo plugin Runtime bootstrap.
//
// Started by the Rust Supervisor as: `node bootstrap.mjs`, cwd = the plugin's read-only
// install directory. The first stdin line is a JSON handshake descriptor (never argv/env):
//
//   { socketPath, token, pluginId, mainPath, dataPath, nodeVersion }
//
// Speaks `u32 BE length + UTF-8 JSON` framed protocol. Private UI↔Runtime uses Electron-style
// ipc (invoke/handle, send/on); external Action/Hook/MCP use commands.register.

import net from "node:net";
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

function log(level, message) {
  send({ type: "log", level, message: String(message) });
}

// -- Length-prefixed JSON framing -------------------------------------------------------

let socket;
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

// -- External commands + Electron-style IPC ---------------------------------------------

const commands = new Map();
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

function ipcHandle(channel, handler) {
  if (typeof channel !== "string" || !channel) {
    throw new TypeError("ipc.handle requires a non-empty string channel");
  }
  if (typeof handler !== "function") {
    throw new TypeError("ipc.handle requires a handler function");
  }
  ipcHandlers.set(channel, handler);
}

function ipcOn(channel, listener) {
  if (typeof channel !== "string" || !channel) {
    throw new TypeError("ipc.on requires a non-empty string channel");
  }
  if (typeof listener !== "function") {
    throw new TypeError("ipc.on requires a listener function");
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
      `ipc.send failed to serialize args for ${channel}: ${error && error.message ? error.message : error}`,
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
    const event = { sender: "ui" };
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
  const event = { sender: "ui" };
  for (const listener of ipcSendListeners.get(channel) ?? []) {
    try {
      listener(event, ...args);
    } catch (error) {
      log("warn", `ipc.on handler failed for ${channel}: ${error}`);
    }
  }
}

function handleCancel(message) {
  const controller = activeInvocations.get(message.id);
  if (controller) controller.abort();
}

async function handleShutdown() {
  try {
    if (typeof pluginModule?.deactivate === "function") {
      await Promise.race([
        Promise.resolve(pluginModule.deactivate()),
        new Promise((resolve) => setTimeout(resolve, COMMAND_GRACE_MS)),
      ]);
    }
  } catch (error) {
    log("warn", `deactivate() threw: ${error}`);
  } finally {
    process.exit(0);
  }
}

function handleHostFrame(message) {
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

function onRuntimeEvent(event, handler) {
  if (!runtimeEventListeners.has(event)) runtimeEventListeners.set(event, new Set());
  runtimeEventListeners.get(event).add(handler);
  return () => runtimeEventListeners.get(event)?.delete(handler);
}

function dispatchRuntimeEvent(event, payload) {
  const name = String(event ?? "");
  for (const handler of runtimeEventListeners.get(name) ?? []) {
    try {
      handler(payload ?? null);
    } catch (error) {
      log("warn", `runtime event handler failed for ${name}: ${error}`);
    }
  }
}

// -- ExtensionContext -------------------------------------------------------------------

function buildHostProxy() {
  return {
    mainPanel: {
      hide: () => callHost("mainPanel.hide", {}),
    },
    app: {
      open: (appId, params) => callHost("app.open", { appId, params: params ?? null }),
    },
    external: {
      open: (url) => callHost("external.open", { url }),
    },
    notify: {
      show: (options) => callHost("notify.show", options ?? {}),
    },
    theme: {
      get: () => callHost("theme.get", {}),
    },
    storage: {
      plugin: {
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
      },
    },
  };
}

function buildContext(descriptor) {
  const ipc = {
    handle: ipcHandle,
    on: ipcOn,
    send: ipcSendToUi,
  };
  return {
    pluginId: descriptor.pluginId,
    registerCommand,
    host: buildHostProxy(),
    ipc,
    ui: {
      /** @deprecated Prefer ctx.ipc.send */
      emit: (event, payload) => ipcSendToUi(event, payload),
    },
    paths: {
      data: descriptor.dataPath,
    },
    runtime: {
      nodeVersion: descriptor.nodeVersion,
    },
    on: onRuntimeEvent,
  };
}

// -- Boot sequence ------------------------------------------------------------------------

let pluginModule;

async function readHandshakeDescriptor() {
  return new Promise((resolve, reject) => {
    let buffer = "";
    function onData(chunk) {
      buffer += chunk.toString("utf8");
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex === -1) return;
      process.stdin.off("data", onData);
      process.stdin.off("error", onError);
      const line = buffer.slice(0, newlineIndex);
      try {
        resolve(JSON.parse(line));
      } catch (error) {
        reject(error);
      }
    }
    function onError(error) {
      reject(error);
    }
    process.stdin.on("data", onData);
    process.stdin.on("error", onError);
  });
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

  socket = net.createConnection(descriptor.socketPath);
  await new Promise((resolve, reject) => {
    socket.once("connect", resolve);
    socket.once("error", reject);
  });
  socket.on("data", onSocketData);
  socket.on("error", (error) => log("warn", `ipc socket error: ${error}`));
  socket.on("close", () => process.exit(0));

  send({ type: "handshake", token: descriptor.token });
  await new Promise((resolve) => setTimeout(resolve, 50));
  if (socket.destroyed) {
    console.error("handshake rejected by host");
    process.exit(1);
    return;
  }

  const ctx = buildContext(descriptor);

  try {
    pluginModule = await import(pathToFileURL(descriptor.mainPath).href);
    const entry =
      pluginModule &&
      typeof pluginModule.default === "object" &&
      pluginModule.default &&
      typeof pluginModule.default.activate === "function"
        ? pluginModule.default
        : pluginModule;
    if (typeof entry.activate !== "function") {
      throw new Error("plugin main does not export an activate(ctx) function");
    }
    pluginModule = entry;
    await pluginModule.activate(ctx);
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

main();
