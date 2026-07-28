import { definePlugin } from "./vendor/tempo-sdk.mjs";
import fs from "node:fs/promises";
import path from "node:path";

function readPluginSettings(settings) {
  const loud = Boolean(settings.loud);
  const defaultWho =
    typeof settings["default-who"] === "string" && settings["default-who"].trim()
      ? settings["default-who"].trim()
      : "world";
  const theme =
    typeof settings.theme === "string" && settings.theme.trim()
      ? settings.theme.trim()
      : "auto";
  const langs = Array.isArray(settings.langs)
    ? settings.langs.filter((item) => typeof item === "string" && item.trim())
    : ["zh"];
  return { loud, defaultWho, theme, langs };
}

export default definePlugin({
  async activate(tempo) {
    tempo.commands.register("hello", async (params) => {
      const settings = await tempo.settings.getAll();
      const { loud, defaultWho, theme, langs } = readPluginSettings(settings);
      const who =
        typeof params?.who === "string" && params.who.trim() ? params.who.trim() : defaultWho;
      const greeting = loud ? `HELLO, ${who.toUpperCase()}!` : `Hello, ${who}!`;
      const langLabel = langs.length > 0 ? langs.join("+") : "none";
      const timestamp = new Date().toISOString();
      const line = `${greeting} [theme=${theme}; langs=${langLabel}] (${timestamp})\n`;

      const logPath = path.join(tempo.paths.data, "hello.log");
      await fs.appendFile(logPath, line, "utf8");

      await tempo.notify.show({
        title: loud ? "HELLO 示例插件" : "Hello 示例插件",
        body: `${greeting} · 主题 ${theme} · 语言 ${langLabel}`,
      });

      tempo.ui.emit("greeted", { who, timestamp, loud, theme, langs });
      return { who, timestamp, logPath, loud, theme, langs, greeting };
    });

    tempo.settings.subscribe((values) => {
      console.log("[hello] settings changed", values);
    });
  },
});
