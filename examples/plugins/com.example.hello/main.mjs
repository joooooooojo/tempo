import { definePlugin } from "./vendor/tempo-sdk.mjs";
import fs from "node:fs/promises";
import path from "node:path";

export default definePlugin({
  async activate(tempo) {
    tempo.commands.register("hello", async (params) => {
      const settings = await tempo.settings.getAll();
      const loud = Boolean(settings.loud);
      const defaultWho =
        typeof settings["default-who"] === "string" && settings["default-who"].trim()
          ? settings["default-who"].trim()
          : "world";
      const who =
        typeof params?.who === "string" && params.who.trim() ? params.who.trim() : defaultWho;
      const greeting = loud ? `HELLO, ${who.toUpperCase()}!` : `Hello, ${who}!`;
      const timestamp = new Date().toISOString();
      const line = `${greeting} (${timestamp})\n`;

      const logPath = path.join(tempo.paths.data, "hello.log");
      await fs.appendFile(logPath, line, "utf8");

      await tempo.notify.show({
        title: "Hello 示例插件",
        body: `已问候 ${who}，记录写入 ${logPath}`,
      });

      tempo.ui.emit("greeted", { who, timestamp, loud });
      return { who, timestamp, logPath, loud };
    });

    tempo.settings.subscribe((values) => {
      console.log("[hello] settings changed", values);
    });
  },
});
