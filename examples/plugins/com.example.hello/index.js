import { createPluginClient } from "./vendor/tempo-sdk.mjs";

(async function () {
  "use strict";

  const whoInput = document.getElementById("who");
  const goButton = document.getElementById("go");
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

  goButton.addEventListener("click", async () => {
    goButton.disabled = true;
    try {
      const result = await tempo.invoke("hello", { who: whoInput.value });
      appendLog(
        `Runtime：${result.greeting ?? result.who} · theme=${result.theme} · langs=${(result.langs ?? []).join("+")} @ ${result.timestamp}`
      );
      await tempo.session.push({ who: whoInput.value });
    } catch (error) {
      appendLog(`失败：${error.message ?? error}`);
    } finally {
      goButton.disabled = false;
    }
  });

  tempo.on("greeted", (payload) => {
    appendLog(
      `收到 Runtime 事件 greeted：${payload.who} · loud=${payload.loud} · theme=${payload.theme}`
    );
  });
})();
