onMounted(() => {
  ipcMain.handle("greet", async (event, input: { name?: string } = {}) => {
    const payload = { message: `Hello, ${input.name || "Tempo"}!` };
    event.sender.send("greeted", payload);
    return payload;
  });

  tempo.commands.register("greet", async (input: { name?: string } = {}) => {
    return { message: `Hello, ${input.name || "Tempo"}!` };
  });

  tempo.mcpTools.register("greet-tool", async (input: { name?: string } = {}) => {
    return { message: `Hello, ${input.name || "Tempo"}!` };
  });
});

onUnmounted(() => {
  console.log("__PLUGIN_NAME__ Runtime unmounted");
});
