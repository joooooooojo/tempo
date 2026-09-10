import { defineRuntime } from "@tempo/sdk/runtime";

defineRuntime(({ commands, events, mcpTools, notify, onDispose }) => {
  commands.register("run", async (params) => {
    await notify.show({
      title: "__PLUGIN_NAME__",
      body: "Headless Command 已执行",
    });
    return { ok: true, params };
  });

  mcpTools.register("run-tool", async (params) => {
    return { ok: true, params };
  });

  const offClipboard = events.on("clipboard.changed", (payload) => {
    console.log("clipboard.changed", payload);
  });

  onDispose(offClipboard);
  onDispose(() => {
    console.log("__PLUGIN_NAME__ Runtime unmounted");
  });
});
