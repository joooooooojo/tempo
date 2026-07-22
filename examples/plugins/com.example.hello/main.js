// com.example.hello — Phase 1 example plugin main entry (package-root main.mjs).
//
// A single bundled ESM with full Node/system access (design §3.2): this is what makes the
// Runtime meaningfully different from the UI, which only ever gets `host.*`/`runtime.*`.
import fs from "node:fs/promises";
import path from "node:path";

export async function activate(ctx) {
  ctx.registerCommand("hello", async (params) => {
    const who = typeof params?.who === "string" && params.who.trim() ? params.who.trim() : "World";
    const timestamp = new Date().toISOString();
    const line = `Hello, ${who}! (${timestamp})\n`;

    // Full filesystem access lives here, in the Runtime — never in the UI (design §2.3).
    const logPath = path.join(ctx.paths.data, "hello.log");
    await fs.appendFile(logPath, line, "utf8");

    await ctx.host.notify.show({
      title: "Hello 示例插件",
      body: `已问候 ${who}，记录写入 ${logPath}`,
    });

    // Runtime -> UI event: any UI instance for this plugin still open receives this via
    // `runtime.on("greeted", ...)` on the client side (relayed as postMessage by the host).
    ctx.ui.emit("greeted", { who, timestamp });

    return { who, timestamp, logPath };
  });
}

export async function deactivate() {
  // Nothing to release for this example — a real plugin would close sockets/handles here.
}
