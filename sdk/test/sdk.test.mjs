import assert from "node:assert/strict";
import test from "node:test";

import { defineRuntime } from "../dist/runtime.js";
import { connect } from "../dist/ui.js";

const HOST_GLOBALS = ["tempo", "ipcRenderer", "ipcMain", "onMounted", "onUnmounted"];

function clearHostGlobals() {
  for (const name of HOST_GLOBALS) delete globalThis[name];
}

test.afterEach(clearHostGlobals);

test("connect waits for the UI context and returns a connected client", async () => {
  let resolveReady;
  const context = { apiVersion: "2.1.0", theme: "dark", params: null, session: null };
  const ready = new Promise((resolve) => {
    resolveReady = resolve;
  });
  const calls = [];
  globalThis.tempo = {
    context: null,
    ready: () => ready,
    storage: { get: async () => null },
    notify: { show: async () => undefined },
  };
  globalThis.ipcRenderer = {
    invoke: async (channel, ...args) => {
      calls.push([channel, args]);
      return { ok: true };
    },
    send() {},
    on() { return () => {}; },
  };

  let connected = false;
  const connecting = connect().then((client) => {
    connected = true;
    return client;
  });
  await Promise.resolve();
  assert.equal(connected, false);

  resolveReady(context);
  const client = await connecting;
  assert.equal(client.context, context);
  assert.equal(client.storage, globalThis.tempo.storage);
  assert.equal("ready" in client, false);
  assert.equal("tempo" in client, false);
  assert.deepEqual(await client.ipc.invoke("probe", 1), { ok: true });
  assert.deepEqual(calls, [["probe", [1]]]);
});

test("connect rejects an invalid host context", async () => {
  globalThis.tempo = { ready: async () => null };
  globalThis.ipcRenderer = {};
  await assert.rejects(connect(), /host returned an invalid UI context/);
});

test("defineRuntime runs setup on mount and disposers in reverse order", async () => {
  const mounted = [];
  const unmounted = [];
  const disposed = [];
  globalThis.tempo = {
    pluginId: "com.example.test",
    commands: {},
    events: {},
  };
  globalThis.ipcMain = {
    handle() {},
    on() { return () => {}; },
    send() {},
  };
  globalThis.onMounted = (hook) => mounted.push(hook);
  globalThis.onUnmounted = (hook) => unmounted.push(hook);

  defineRuntime(({ onDispose, pluginId }) => {
    assert.equal(pluginId, "com.example.test");
    onDispose(() => disposed.push("first"));
    onDispose(async () => disposed.push("second"));
    return () => disposed.push("returned");
  });

  assert.equal(mounted.length, 1);
  assert.equal(unmounted.length, 1);
  assert.deepEqual(disposed, []);
  await mounted[0]();
  await unmounted[0]();
  assert.deepEqual(disposed, ["returned", "second", "first"]);
});

test("defineRuntime attempts every disposer when cleanup fails", async () => {
  const mounted = [];
  const unmounted = [];
  const disposed = [];
  globalThis.tempo = {};
  globalThis.ipcMain = { handle() {}, on() { return () => {}; }, send() {} };
  globalThis.onMounted = (hook) => mounted.push(hook);
  globalThis.onUnmounted = (hook) => unmounted.push(hook);

  defineRuntime(({ onDispose }) => {
    onDispose(() => disposed.push("first"));
    onDispose(() => { throw new Error("cleanup failed"); });
    onDispose(() => disposed.push("last"));
  });
  await mounted[0]();
  await assert.rejects(unmounted[0](), AggregateError);
  assert.deepEqual(disposed, ["last", "first"]);
});

test("defineRuntime cleans up a partially initialized setup", async () => {
  const mounted = [];
  const unmounted = [];
  const disposed = [];
  globalThis.tempo = {};
  globalThis.ipcMain = { handle() {}, on() { return () => {}; }, send() {} };
  globalThis.onMounted = (hook) => mounted.push(hook);
  globalThis.onUnmounted = (hook) => unmounted.push(hook);

  defineRuntime(({ onDispose }) => {
    onDispose(() => disposed.push("released"));
    throw new Error("setup failed");
  });

  await assert.rejects(mounted[0](), /setup failed/);
  assert.deepEqual(disposed, ["released"]);
  await unmounted[0]();
});

test("SDK entry points report missing host globals clearly", async () => {
  clearHostGlobals();
  await assert.rejects(connect(), /host global "tempo" is unavailable/);
  assert.throws(
    () => defineRuntime(() => {}),
    /host global "tempo" is unavailable/,
  );
});
