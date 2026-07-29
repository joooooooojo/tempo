import { createPluginClient } from "./vendor/tempo-sdk.mjs";

(async function () {
  "use strict";

  const whoInput = document.getElementById("who");
  const goButton = document.getElementById("go");
  const scaButton = document.getElementById("sca");
  const logEl = document.getElementById("log");
  const themeEl = document.getElementById("theme");
  const pluginSettingsEl = document.getElementById("plugin-settings");

  function appendLog(line) {
    logEl.textContent = `${line}\n${logEl.textContent}`.trim();
  }

  function formatPluginSettings(values) {
    const loud = Boolean(values.loud);
    const theme = typeof values.theme === "string" ? values.theme : "auto";
    const langs = Array.isArray(values.langs) ? values.langs.join("+") : "zh";
    const defaultWho =
      typeof values["default-who"] === "string" && values["default-who"].trim()
        ? values["default-who"].trim()
        : "world";
    return `插件配置：loud=${loud} · theme=${theme} · langs=${langs} · default-who=${defaultWho}`;
  }

  function buildScaFixture(label) {
    const bytes = new Uint8Array([9, 8, 7, 6]);
    const fixture = {
      label: String(label ?? "ui"),
      at: new Date("2025-01-02T03:04:05.000Z"),
      tags: new Set(["ui", "probe"]),
      meta: new Map([
        ["from", "ui"],
        ["n", 42],
      ]),
      bytes,
    };
    fixture.self = fixture;
    return fixture;
  }

  function describeScaValue(value, prefix = "value") {
    if (!(value && typeof value === "object")) return [`${prefix}: FAIL not object`];
    const lines = [];
    const dateOk = value.at instanceof Date;
    const setOk = value.tags instanceof Set;
    const mapOk = value.meta instanceof Map;
    const bytesOk = value.bytes instanceof Uint8Array;
    const cycleOk = value.self === value;
    lines.push(
      `${prefix}: Date=${dateOk ? "ok" : "FAIL"} Set=${setOk ? "ok" : "FAIL"} Map=${mapOk ? "ok" : "FAIL"} Uint8Array=${bytesOk ? "ok" : "FAIL"} cycle=${cycleOk ? "ok" : "FAIL"}`,
    );
    if (dateOk) lines.push(`  at=${value.at.toISOString()}`);
    if (setOk) lines.push(`  tags=[${[...value.tags].join(",")}]`);
    if (mapOk) lines.push(`  meta.from=${value.meta.get("from") ?? value.meta.get("version")}`);
    if (bytesOk) lines.push(`  bytes=[${Array.from(value.bytes).join(",")}]`);
    return lines;
  }

  function allScaOk(value) {
    return (
      value &&
      typeof value === "object" &&
      value.at instanceof Date &&
      value.tags instanceof Set &&
      value.meta instanceof Map &&
      value.bytes instanceof Uint8Array &&
      value.self === value
    );
  }

  const tempo = await createPluginClient();
  const context = tempo.context;
  themeEl.textContent = `宿主主题：${context.theme} · API v${context.apiVersion}`;

  const settings = await tempo.settings.getAll();
  pluginSettingsEl.textContent = formatPluginSettings(settings);
  const defaultWho =
    typeof settings["default-who"] === "string" && settings["default-who"].trim()
      ? settings["default-who"].trim()
      : "world";

  if (context.session && typeof context.session.who === "string") {
    whoInput.value = context.session.who;
  } else {
    whoInput.value = defaultWho;
  }

  if (context.params?.input?.kind === "text") {
    whoInput.value = context.params.input.text;
    appendLog(`Action 注入文本：${context.params.input.text}`);
  } else if (context.params?.input?.kind === "image") {
    const { width, height } = context.params.input;
    appendLog(`Action 注入图片：${width ?? "?"} x ${height ?? "?"}`);
  } else if (context.params?.input?.kind === "file") {
    const paths = context.params.input.paths ?? [];
    appendLog(`Action 注入文件：${paths.join(", ") || "(空)"}`);
  }

  tempo.settings.subscribe((values) => {
    pluginSettingsEl.textContent = formatPluginSettings(values);
    appendLog(`配置已更新：${JSON.stringify(values)}`);
    if (
      typeof values["default-who"] === "string" &&
      values["default-who"].trim() &&
      !whoInput.value.trim()
    ) {
      whoInput.value = values["default-who"].trim();
    }
  });

  void tempo.theme.subscribe((theme) => {
    themeEl.textContent = `宿主主题：${theme} · API v${context.apiVersion}`;
  });

  tempo.ipc.on("greeted", (_event, payload) => {
    const dateOk = payload?.at instanceof Date;
    appendLog(
      `on greeted：who=${payload?.who} Date=${dateOk ? "ok " + payload.at.toISOString() : "FAIL"} loud=${payload?.loud}`,
    );
  });

  tempo.ipc.on("sca-echo", (_event, payload) => {
    for (const line of describeScaValue(payload, "on sca-echo")) appendLog(line);
    appendLog(allScaOk(payload) ? "✓ sca-echo SCA 通过" : "✗ sca-echo SCA 失败");
  });

  tempo.ipc.on("sca-pong", (_event, payload) => {
    for (const line of describeScaValue(payload, "on sca-pong")) appendLog(line);
    appendLog(allScaOk(payload) ? "✓ sca-pong SCA 通过" : "✗ sca-pong SCA 失败");
  });

  goButton.addEventListener("click", async () => {
    goButton.disabled = true;
    try {
      const result = await tempo.ipc.invoke("greet", { who: whoInput.value });
      const dateOk = result?.at instanceof Date;
      appendLog(
        `invoke greet：${result.greeting ?? result.who} · Date=${dateOk ? "ok " + result.at.toISOString() : "FAIL"} · langs=${(result.langs ?? []).join("+")}`,
      );
      await tempo.session.push({ who: whoInput.value });
    } catch (error) {
      appendLog(`失败：${error.message ?? error}`);
    } finally {
      goButton.disabled = false;
    }
  });

  scaButton.addEventListener("click", async () => {
    scaButton.disabled = true;
    try {
      const outgoing = buildScaFixture("ui-probe");
      appendLog("— SCA 探测开始 —");
      for (const line of describeScaValue(outgoing, "ui→rt invoke args")) appendLog(line);

      const result = await tempo.ipc.invoke("sca-probe", outgoing);
      appendLog(`invoke sca-probe ok=${result?.ok}`);
      for (const line of result?.checks ?? []) appendLog(`  rt: ${line}`);
      for (const line of describeScaValue(result?.outgoing, "invoke result.outgoing")) {
        appendLog(line);
      }
      appendLog(
        allScaOk(result?.outgoing) ? "✓ invoke/handle SCA 通过" : "✗ invoke/handle SCA 失败",
      );

      // Fire-and-forget send; pong arrives via ipc.on("sca-pong").
      tempo.ipc.send("sca-ping", buildScaFixture("ui-ping"));
      appendLog("send sca-ping 已发出（等待 sca-pong）");
    } catch (error) {
      appendLog(`SCA 探测失败：${error.message ?? error}`);
    } finally {
      scaButton.disabled = false;
    }
  });
})();
