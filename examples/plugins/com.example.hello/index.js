// com.example.hello — UI. Host auto-injects `window.plugin` (no SDK).
(async function () {
  "use strict";

  const whoInput = document.getElementById("who");
  const goButton = document.getElementById("go");
  const logEl = document.getElementById("log");
  const themeEl = document.getElementById("theme");

  function appendLog(line) {
    logEl.textContent = `${line}\n${logEl.textContent}`.trim();
  }

  const context = await window.plugin.ready();
  themeEl.textContent = `主题：${context.theme} · API v${context.apiVersion}`;
  if (context.session && typeof context.session.who === "string") {
    whoInput.value = context.session.who;
  }

  goButton.addEventListener("click", async () => {
    goButton.disabled = true;
    try {
      const result = await window.plugin.invoke("hello", { who: whoInput.value });
      appendLog(`Runtime 已记录：${result.who} @ ${result.timestamp}`);
      await window.plugin.host("session.push", { payload: { who: whoInput.value } });
    } catch (error) {
      appendLog(`失败：${error.message ?? error}`);
    } finally {
      goButton.disabled = false;
    }
  });

  window.plugin.on("greeted", (payload) => {
    appendLog(`收到 Runtime 事件 greeted：${payload.who}`);
  });
})();
