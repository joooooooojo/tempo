onMounted(() => {
  tempo.commands.register("run", async (params) => {
    await tempo.notify.show({
      title: "__PLUGIN_NAME__",
      body: "Headless Command 已执行",
    });
    return { ok: true, params };
  });

  tempo.mcpTools.register("run-tool", async (params) => {
    return { ok: true, params };
  });

  tempo.events.on("clipboard.changed", (payload) => {
    console.log("clipboard.changed", payload);
  });
});

onUnmounted(() => {
  console.log("__PLUGIN_NAME__ Runtime unmounted");
});
