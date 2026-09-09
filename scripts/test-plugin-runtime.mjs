import assert from "node:assert/strict";
import net from "node:net";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { mkdtemp, rm } from "node:fs/promises";
import { build } from "vite";

const bootstrap = fileURLToPath(new URL("../core/plugin-runtime/bootstrap.mjs", import.meta.url));
const mainPath = fileURLToPath(new URL("./fixtures/plugin-runtime-smoke.mjs", import.meta.url));
const codec = fileURLToPath(new URL("../core/plugin-runtime/structured-clone.mjs", import.meta.url));
const deno = process.env.TEMPO_PLUGIN_DENO_PATH;

function encode(value) {
  const body = Buffer.from(JSON.stringify(value));
  const frame = Buffer.alloc(body.length + 4);
  frame.writeUInt32BE(body.length);
  body.copy(frame, 4);
  return frame;
}

async function exercise(t, { engine = "node", legacy = false, mode = "success", bundled = false, grantData = false } = {}) {
  const scratch = await mkdtemp(path.join(tmpdir(), "tempo-deno-test-"));
  t.after(() =>
    rm(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }),
  );
  let entryPath = mainPath;
  if (bundled) {
    await build({ configFile: false, logLevel: "silent", build: { ssr: fileURLToPath(new URL("./fixtures/plugin-runtime-bundle.mjs", import.meta.url)), outDir: path.join(scratch, "dist"), rollupOptions: { output: { entryFileNames: "main.mjs" } } }, ssr: { noExternal: true } });
    entryPath = path.join(scratch, "dist", "main.mjs");
  }
  const sockets = new Set();
  const timers = new Set();
  const frames = [];
  const hostFiles = new Map();
  let acknowledged = false;
  let premature = false;
  const pending = new Set(["probe", "mcp", "ipc"]);
  const server = net.createServer((socket) => {
    sockets.add(socket);
    socket.on("error", () => {});
    let buffer = Buffer.alloc(0);
    socket.on("data", (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      while (buffer.length >= 4 && buffer.length >= 4 + buffer.readUInt32BE(0)) {
        const length = buffer.readUInt32BE(0);
        const frame = JSON.parse(buffer.subarray(4, 4 + length));
        buffer = buffer.subarray(4 + length);
        frames.push(frame);
        if (frame.type === "handshake") {
          if (mode === "disconnect") { socket.destroy(); continue; }
          if (mode === "timeout") continue;
          const timer = setTimeout(() => {
            acknowledged = true;
            socket.write(encode({ type: "response", id: "handshake", ok: mode !== "reject", result: {} }));
          }, 150);
          timers.add(timer);
        } else if (frame.type === "ready") {
          premature ||= !acknowledged;
          socket.write(encode({ type: "invoke", id: "probe", commandId: "probe", params: { runExecutable: process.execPath } }));
          socket.write(encode({ type: "mcp-invoke", id: "mcp", toolName: "echo", arguments: { value: 42 } }));
          socket.write(encode({ type: "ipc-invoke", id: "ipc", channel: "echo", args: ["from-ui"] }));
        } else if (frame.type === "request") {
          let result = null;
          if (frame.method === "files.mkdir") {
            result = null;
          } else if (frame.method === "files.writeText") {
            hostFiles.set(frame.params.path, frame.params.base64);
          } else if (frame.method === "files.readText") {
            result = { base64: hostFiles.get(frame.params.path) };
          } else if (frame.method === "files.writeBytes") {
            hostFiles.set(frame.params.path, frame.params.base64);
          } else if (frame.method === "files.readBytes") {
            result = { base64: hostFiles.get(frame.params.path) };
          } else {
            throw new Error("unexpected Host method " + frame.method);
          }
          socket.write(encode({ type: "response", id: frame.id, ok: true, result }));
        } else if (frame.type === "response" && pending.delete(frame.id)) {
          if (pending.size === 0) socket.write(encode({ type: "shutdown" }));
        }
      }
    });
  });
  t.after(() => {
    for (const timer of timers) clearTimeout(timer);
    for (const socket of sockets) socket.destroy();
    server.close();
  });
  const pipe = process.platform === "win32"
    ? `\\\\.\\pipe\\tempo-runtime-test-${randomUUID()}`
    : path.join(tmpdir(), `tempo-${randomUUID()}.sock`);
  if (legacy) server.listen(pipe);
  else server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  const descriptor = {
    token: "test-token", pluginId: "test.runtime", mainPath: entryPath,
    dataPath: scratch, runtimeVersion: "2.9.6",
    ...(legacy ? { socketPath: pipe } : { socketAddress: { host: "127.0.0.1", port: address.port } }),
  };
  if (mode === "invalid") descriptor.socketAddress.host = "192.0.2.1";
  if (mode === "ambiguous") descriptor.socketPath = pipe;
  const env = { DENO_DIR: path.join(scratch, "cache") };
  for (const key of ["SystemRoot", "WINDIR", "TEMP", "TMP"]) if (process.env[key]) env[key] = process.env[key];
  const args = engine === "deno"
    ? ["run", "--no-config", "--no-lock", "--no-prompt", "--cached-only", "--no-remote", "--no-npm", "--node-modules-dir=none", `--allow-net=127.0.0.1:${address.port}`, `--allow-read=${bundled ? path.dirname(entryPath) : `${mainPath},${codec}`}${grantData ? `,${scratch}` : ""}`, ...(grantData ? [`--allow-write=${scratch}`] : []), bootstrap]
    : [bootstrap];
  const child = spawn(engine === "deno" ? deno : process.execPath, args, { env, stdio: ["pipe", "pipe", "pipe"] });
  const watchdog = setTimeout(() => child.kill(), 9000);
  t.after(() => { clearTimeout(watchdog); if (child.exitCode === null) child.kill(); });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  child.stdout.resume();
  child.stdin.on("error", () => {});
  child.stdin.end(mode === "eof" ? "{" : JSON.stringify(descriptor) + "\n");
  const [code, signal] = await once(child, "close");
  assert.equal(signal, null, stderr);
  if (mode !== "success") {
    assert.equal(code, 1, stderr);
    assert.equal(frames.some((frame) => frame.type === "ready"), false);
    return;
  }
  assert.equal(code, 0, stderr);
  assert.equal(premature, false, "plugin activated before host acknowledgement");
  assert.equal(frames.find((frame) => frame.type === "ready")?.ok, true, stderr);
  const response = frames.find((frame) => frame.id === "probe");
  assert.equal(response?.ok, true, JSON.stringify(response ?? stderr));
  assert.equal(response.result.cycle, true);
  assert.equal(response.result.date, "2026-01-01T00:00:00.000Z");
  assert.deepEqual(response.result.bytes, [111, 107]);
  assert.equal(response.result.envelope, "string");
  assert.equal(response.result.hostFileText, "host-api");
  assert.deepEqual(response.result.hostFileBytes, [0, 1, 255]);
  assert.equal(frames.find(frame => frame.id === "mcp")?.ok, true);
  assert.equal(frames.find(frame => frame.id === "mcp")?.result.value, 42);
  assert.equal(frames.find(frame => frame.id === "ipc")?.ok, true);
  if (engine === "deno") {
    assert.equal(response.result.deniedRead, !grantData);
    assert.equal(response.result.dataWrite, grantData);
    for (const permission of ["deniedEnv", "deniedRun", "deniedNet", "deniedFfi", "deniedRemoteImport", "deniedNpmImport"]) assert.equal(response.result[permission], true, permission);
    assert.equal(response.result.runtime.engine, "deno");
    assert.equal(response.result.runtime.version, "2.9.6");
  }
}

test("Node TCP activation, delayed ack, command and structured clone", (t) => exercise(t));
test("Node legacy IPC remains compatible", (t) => exercise(t, { legacy: true }));
for (const mode of ["reject", "disconnect", "invalid", "ambiguous", "eof", "timeout"]) {
  test(`startup fails closed: ${mode}`, (t) => exercise(t, { mode }));
}
test("Deno restricted TCP activation and filesystem denial", { skip: !deno }, (t) => exercise(t, { engine: "deno" }));
test("Node/Vite bundled npm dependency executes on Deno with data grants", { skip: !deno }, (t) => exercise(t, { engine: "deno", bundled: true, grantData: true }));
for (const mode of ["reject", "disconnect", "invalid", "ambiguous", "eof", "timeout"]) {
  test(`Deno startup fails closed: ${mode}`, { skip: !deno }, (t) => exercise(t, { engine: "deno", mode }));
}
