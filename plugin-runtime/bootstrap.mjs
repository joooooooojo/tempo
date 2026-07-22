#!/usr/bin/env node
// Tempo plugin Runtime bootstrap (design §6.3, §7).
//
// Started by the Rust Supervisor as: `node bootstrap.mjs`, cwd = the plugin's read-only
// install directory. The first stdin line is a JSON handshake descriptor (never argv/env):
//
//   { socketPath, token, pluginId, mainPath, dataPath, nodeVersion }
//
// This process then connects to `socketPath` (Unix domain socket / Windows named pipe — the
// `node:net` module handles both transparently via a path string), sends
// `{ type: "handshake", token }`, and speaks the same `u32 BE length + UTF-8 JSON` framed
// protocol as the host for the rest of its life. It loads exactly one plugin `main` bundle
// and never proxies to another plugin.

import net from "node:net";
import { randomUUID } from "node:crypto";

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
  } catch (error) {
    // The host connection is gone; nothing useful to do but let the process exit naturally
    // once the host kills the process tree.
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

// -- Runtime -> host: registered commands ------------------------------------------------

const commands = new Map();
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

async function handleInvoke(message) {
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
    // Best-effort notice only; the host Supervisor is the real enforcer of process death.
    log("warn", `command ${commandId} exceeded grace period after abort`);
  }, COMMAND_TIMEOUT_MS + COMMAND_GRACE_MS);

  try {
    const result = await handler(params, controller.signal);
    send({ type: "response", id, ok: true, result: result === undefined ? null : result });
  } catch (error) {
    if (controller.signal.aborted) {
      send({ type: "response", id, ok: false, error: { code: "TIMEOUT", message: "command timed out" } });
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
      void handleInvoke(message);
      return;
    case "cancel":
      handleCancel(message);
      return;
    case "shutdown":
      void handleShutdown();
      return;
    default:
      return;
  }
}

// -- ExtensionContext (design §6.3, §7) --------------------------------------------------

function buildHostProxy() {
  const namespaces = [
    "palette",
    "app",
    "external",
    "notify",
    "theme",
    "subscription",
    "storage",
  ];
  const host = {};
  for (const ns of namespaces) {
    host[ns] = new Proxy(
      {},
      {
        get(_target, methodName) {
          if (typeof methodName !== "string") return undefined;
          return (params) => callHost(`${ns}.${methodName}`, params ?? {});
        },
      }
    );
  }
  // storage.plugin.get/set/delete/list
  host.storage = { plugin: {
    get: (key) => callHost("storage.plugin.get", { key }),
    set: (key, value) => callHost("storage.plugin.set", { key, value }),
    delete: (key) => callHost("storage.plugin.delete", { key }),
    list: () => callHost("storage.plugin.list", {}),
  } };
  return host;
}

function buildContext(descriptor) {
  return {
    pluginId: descriptor.pluginId,
    registerCommand,
    host: buildHostProxy(),
    ui: {
      emit(event, payload) {
        send({ type: "event", event: String(event), payload: payload ?? null });
      },
    },
    paths: {
      data: descriptor.dataPath,
    },
    runtime: {
      nodeVersion: descriptor.nodeVersion,
    },
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
  // The host replies with a synthetic `{type:"response", id:"handshake"}` ack; we don't need
  // to block on it before loading the plugin, but we do wait for the socket to flush it so a
  // token mismatch (host destroys the connection) is observed before running plugin code.
  await new Promise((resolve) => setTimeout(resolve, 50));
  if (socket.destroyed) {
    console.error("handshake rejected by host");
    process.exit(1);
    return;
  }

  const ctx = buildContext(descriptor);

  try {
    pluginModule = await import(pathToFileUrl(descriptor.mainPath));
    if (typeof pluginModule.activate !== "function") {
      throw new Error("plugin main does not export an activate(ctx) function");
    }
    await pluginModule.activate(ctx);
    send({ type: "ready", ok: true });
  } catch (error) {
    send({
      type: "ready",
      ok: false,
      error: { code: "ACTIVATION_FAILED", message: error && error.message ? String(error.message) : String(error) },
    });
    // Give the host a moment to read the frame before we exit.
    setTimeout(() => process.exit(1), 100);
  }
}

function pathToFileUrl(path) {
  const normalized = path.replace(/\\/g, "/");
  const prefixed = normalized.startsWith("/") ? normalized : `/${normalized}`;
  return `file://${prefixed}`;
}

process.on("uncaughtException", (error) => {
  log("error", `uncaught exception: ${error && error.stack ? error.stack : error}`);
});
process.on("unhandledRejection", (error) => {
  log("error", `unhandled rejection: ${error && error.stack ? error.stack : error}`);
});

main();
