import { defineRuntime } from "@tempo/sdk/runtime";
import type { PluginIpc } from "../ipc.js";

defineRuntime<PluginIpc>(({ commands, ipc, mcpTools, onDispose }) => {
  ipc.handle("greet", async (event, input = {}) => {
    const payload = { message: `Hello, ${input.name || "Tempo"}!` };
    event.sender.send("greeted", payload);
    return payload;
  });

  commands.register("greet", async (input: { name?: string } = {}) => {
    return { message: `Hello, ${input.name || "Tempo"}!` };
  });

  mcpTools.register("greet-tool", async (input: { name?: string } = {}) => {
    return { message: `Hello, ${input.name || "Tempo"}!` };
  });

  onDispose(() => {
    console.log("__PLUGIN_NAME__ Runtime unmounted");
  });
});
