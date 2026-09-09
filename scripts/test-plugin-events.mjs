import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const source = await readFile(new URL("../core/plugin-ui/bridge-client.js", import.meta.url), "utf8");
const windowListeners = new Map();
const parent = { postMessage() {} };
const window = {
  parent,
  addEventListener(event, handler) {
    if (!windowListeners.has(event)) windowListeners.set(event, new Set());
    windowListeners.get(event).add(handler);
  },
};
const context = vm.createContext({ atob, btoa, console, TextDecoder, TextEncoder, window });
vm.runInContext(source, context, { filename: "core/plugin-ui/bridge-client.js" });

const hostFiles = new Map();
parent.postMessage = (request) => {
  let result = null;
  if (request.method === "files.mkdir") {
    result = null;
  } else if (request.method === "files.writeText") {
    hostFiles.set(request.params.path, request.params.base64);
  } else if (request.method === "files.readText") {
    result = { base64: hostFiles.get(request.params.path) };
  } else if (request.method === "files.writeBytes") {
    hostFiles.set(request.params.path, request.params.base64);
  } else if (request.method === "files.readBytes") {
    result = { base64: hostFiles.get(request.params.path) };
  } else {
    return;
  }
  queueMicrotask(() => {
    for (const handler of windowListeners.get("message") ?? []) {
      handler({
        source: parent,
        data: {
          type: "tempo-plugin-rpc-response",
          id: request.id,
          ok: true,
          result,
        },
      });
    }
  });
};

await window.tempo.files.mkdir("host");
await window.tempo.files.writeText("host/ui.txt", "ui-host-api");
assert.equal(await window.tempo.files.readText("host/ui.txt"), "ui-host-api");
const uiBytes = vm.runInContext("new Uint8Array([0, 1, 255])", context);
await window.tempo.files.writeBytes("host/ui.bin", uiBytes);
assert.deepEqual(
  Array.from(await window.tempo.files.readBytes("host/ui.bin")),
  [0, 1, 255],
);

function dispatch(event, payload) {
  for (const handler of windowListeners.get("message") ?? []) {
    handler({
      source: parent,
      data: { type: "tempo-plugin-event", source: "platform", event, payload },
    });
  }
}

const { events } = window.tempo;
assert.equal(events.emit, undefined);

let persistentCalls = 0;
const persistent = () => {
  persistentCalls += 1;
};
events.on("status.changed", persistent);
assert.equal(events.listenerCount("status.changed"), 1);
assert.deepEqual(Array.from(events.eventNames()), ["status.changed"]);

let onceCalls = 0;
const once = () => {
  onceCalls += 1;
};
events.once("status.changed", once);
assert.equal(events.listenerCount("status.changed"), 2);
dispatch("status.changed", { value: 1 });
dispatch("status.changed", { value: 2 });
assert.equal(persistentCalls, 2);
assert.equal(onceCalls, 1);
assert.equal(events.listenerCount("status.changed"), 1);

assert.equal(events.off("status.changed", persistent), true);
assert.equal(events.off("status.changed", persistent), false);
assert.equal(events.listenerCount("status.changed"), 0);

let cancelledOnceCalls = 0;
const cancelledOnce = () => {
  cancelledOnceCalls += 1;
};
events.once("status.changed", cancelledOnce);
assert.equal(events.off("status.changed", cancelledOnce), true);
dispatch("status.changed", {});
assert.equal(cancelledOnceCalls, 0);

events.on("alpha", () => {});
events.on("beta", () => {});
events.removeAllListeners("alpha");
assert.deepEqual(Array.from(events.eventNames()), ["beta"]);
events.removeAllListeners();
assert.deepEqual(Array.from(events.eventNames()), []);

let settingsCalls = 0;
window.tempo.settings.subscribe(() => {
  settingsCalls += 1;
});
events.on("clipboard.changed", () => {});
events.removeAllListeners();
dispatch("settings.changed", { values: { compact: true } });
assert.equal(settingsCalls, 1);

console.log("tempo.events listener API verified");
