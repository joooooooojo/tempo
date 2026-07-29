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

function buildScaFixture(label) {
  const bytes = new Uint8Array([1, 2, 3, 4]);
  const fixture = {
    label: String(label ?? "fixture"),
    at: new Date("2024-06-15T12:00:00.000Z"),
    tags: new Set(["hello", "sca"]),
    meta: new Map([
      ["version", 2],
      ["ok", true],
    ]),
    bytes,
  };
  fixture.self = fixture;
  return fixture;
}

function describeScaValue(value, prefix = "value") {
  const checks = [];
  if (!(value && typeof value === "object")) {
    return [`${prefix}: not an object`];
  }
  checks.push(
    `${prefix}.at Date=${value.at instanceof Date} iso=${value.at instanceof Date ? value.at.toISOString() : String(value.at)}`,
  );
  checks.push(
    `${prefix}.tags Set=${value.tags instanceof Set} size=${value.tags instanceof Set ? value.tags.size : "?"}`,
  );
  checks.push(
    `${prefix}.meta Map=${value.meta instanceof Map} version=${value.meta instanceof Map ? value.meta.get("version") : "?"}`,
  );
  checks.push(
    `${prefix}.bytes Uint8Array=${value.bytes instanceof Uint8Array} [${value.bytes instanceof Uint8Array ? Array.from(value.bytes).join(",") : "?"}]`,
  );
  checks.push(`${prefix}.self cycle=${value.self === value}`);
  return checks;
}

async function greet(tempo, params) {
  const settings = await tempo.settings.getAll();
  const { loud, defaultWho, theme, langs } = readPluginSettings(settings);
  const who =
    typeof params?.who === "string" && params.who.trim() ? params.who.trim() : defaultWho;
  const greeting = loud ? `HELLO, ${who.toUpperCase()}!` : `Hello, ${who}!`;
  const langLabel = langs.length > 0 ? langs.join("+") : "none";
  const at = new Date();
  const timestamp = at.toISOString();
  const line = `${greeting} [theme=${theme}; langs=${langLabel}] (${timestamp})\n`;

  const logPath = path.join(tempo.paths.data, "hello.log");
  await fs.appendFile(logPath, line, "utf8");

  await tempo.notify.show({
    title: loud ? "HELLO 示例插件" : "Hello 示例插件",
    body: `${greeting} · 主题 ${theme} · 语言 ${langLabel}`,
  });

  // Event payload includes a Date so UI can verify ipc.send → ipc.on SCA.
  tempo.ipc.send("greeted", {
    who,
    at,
    loud,
    theme,
    langs,
  });

  return { who, at, timestamp, logPath, loud, theme, langs, greeting };
}

export default definePlugin({
  async activate(tempo) {
    tempo.ipc.handle("greet", async (_event, params) => greet(tempo, params ?? {}));

    // Dedicated SCA probe: UI → Runtime invoke + Runtime → UI send.
    tempo.ipc.handle("sca-probe", async (_event, incoming) => {
      console.log("[hello] sca-probe received", describeScaValue(incoming, "in").join(" | "));
      const outgoing = buildScaFixture(`echo:${incoming?.label ?? "?"}`);
      tempo.ipc.send("sca-echo", outgoing);
      return {
        ok: true,
        checks: describeScaValue(incoming, "in"),
        outgoing,
      };
    });

    // UI → Runtime fire-and-forget send → on.
    tempo.ipc.on("sca-ping", (_event, incoming) => {
      console.log("[hello] sca-ping received", describeScaValue(incoming, "ping").join(" | "));
      tempo.ipc.send("sca-pong", buildScaFixture(`pong:${incoming?.label ?? "?"}`));
    });

    // Declared external command for Action / MCP only (UI cannot invoke commands).
    tempo.commands.register("hello", async (params) => greet(tempo, params));

    tempo.settings.subscribe((values) => {
      console.log("[hello] settings changed", values);
    });
  },
});
