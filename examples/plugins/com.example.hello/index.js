import { createPluginClient } from "./vendor/tempo-sdk.mjs";

(async function () {
  "use strict";

  const whoInput = document.getElementById("who");
  const goButton = document.getElementById("go");
  const logEl = document.getElementById("log");
  const themeEl = document.getElementById("theme");

  function appendLog(line) {
    logEl.textContent = `${line}\n${logEl.textContent}`.trim();
  }

  const tempo = await createPluginClient();
  const context = tempo.context;
  themeEl.textContent = `主题：${context.theme} · API v${context.apiVersion}`;

  const settings = await tempo.settings.getAll();
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
  }

  tempo.settings.subscribe((values) => {
    appendLog(`配置已更新：${JSON.stringify(values)}`);
  });

  void tempo.theme.subscribe((theme) => {
    themeEl.textContent = `主题：${theme} · API v${context.apiVersion}`;
  });

  goButton.addEventListener("click", async () => {
    goButton.disabled = true;
    try {
      const result = await tempo.invoke("hello", { who: whoInput.value });
      appendLog(`Runtime 已记录：${result.who} @ ${result.timestamp}`);
      await tempo.session.push({ who: whoInput.value });
    } catch (error) {
      appendLog(`失败：${error.message ?? error}`);
    } finally {
      goButton.disabled = false;
    }
  });

  tempo.on("greeted", (payload) => {
    appendLog(`收到 Runtime 事件 greeted：${payload.who}`);
  });
})();
