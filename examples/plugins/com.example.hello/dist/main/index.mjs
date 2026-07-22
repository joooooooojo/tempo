export async function activate(ctx) {
  ctx.registerCommand("hello", async () => {
    const target = `${ctx.paths.data}/last.txt`;
    await import("node:fs/promises").then((fs) =>
      fs.writeFile(target, new Date().toISOString(), "utf8")
    );
    await ctx.host.notify.show({
      title: "Hello",
      body: "from full-runtime plugin",
    });
    return { ok: true };
  });
}

export async function deactivate() {}
