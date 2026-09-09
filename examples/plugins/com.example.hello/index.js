"use strict";

const context = await window.tempo.ready();

  const whoInput = document.getElementById("who");
  const goButton = document.getElementById("go");
  const scaButton = document.getElementById("sca");
  const permissionsButton = document.getElementById("permissions");
  const logEl = document.getElementById("log");
  const themeEl = document.getElementById("theme");
  const pluginSettingsEl = document.getElementById("plugin-settings");
  const permissionPolicyEl = document.getElementById("permission-policy");
  const permissionNames = ["read", "write", "net", "env", "sys", "run", "ffi", "import"];

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

  function setPermissionResult(id, text, status) {
    const element = document.getElementById(`permission-${id}`);
    element.textContent = text;
    element.dataset.status = status;
  }

  function renderQueriedPermission(name, state) {
    if (state === "granted") {
      setPermissionResult(name, "允许", "allowed");
      return;
    }
    if (state === "denied" || state === "prompt") {
      setPermissionResult(name, `已阻止（${state}）`, "blocked");
      return;
    }
    setPermissionResult(name, `无法查询（${state}）`, "error");
  }

  async function runUiFileProbe() {
    const directory = "permissions-demo";
    const textPath = `${directory}/ui.txt`;
    const bytesPath = `${directory}/ui.bin`;
    await window.tempo.files.mkdir(directory, { recursive: true });
    await window.tempo.files.writeText(textPath, "UI host file API works");
    await window.tempo.files.writeBytes(bytesPath, new Uint8Array([3, 2, 1]));
    const text = await window.tempo.files.readText(textPath);
    const bytes = await window.tempo.files.readBytes(bytesPath);
    return text === "UI host file API works" && bytes.join(",") === "3,2,1";
  }

  themeEl.textContent = `宿主主题：${context.theme} · API v${context.apiVersion}`;

  const settings = await window.tempo.settings.getAll();
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

  window.tempo.settings.subscribe((values) => {
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

  void window.tempo.theme.subscribe((theme) => {
    themeEl.textContent = `宿主主题：${theme} · API v${context.apiVersion}`;
  });

  window.ipcRenderer.on("greeted", (_event, payload) => {
    const dateOk = payload?.at instanceof Date;
    appendLog(
      `on greeted：who=${payload?.who} Date=${dateOk ? "ok " + payload.at.toISOString() : "FAIL"} loud=${payload?.loud}`,
    );
  });

  window.ipcRenderer.on("sca-echo", (_event, payload) => {
    for (const line of describeScaValue(payload, "on sca-echo")) appendLog(line);
    appendLog(allScaOk(payload) ? "✓ sca-echo SCA 通过" : "✗ sca-echo SCA 失败");
  });

  window.ipcRenderer.on("sca-pong", (_event, payload) => {
    for (const line of describeScaValue(payload, "on sca-pong")) appendLog(line);
    appendLog(allScaOk(payload) ? "✓ sca-pong SCA 通过" : "✗ sca-pong SCA 失败");
  });

  goButton.addEventListener("click", async () => {
    goButton.disabled = true;
    try {
      const result = await window.ipcRenderer.invoke("greet", { who: whoInput.value });
      const dateOk = result?.at instanceof Date;
      appendLog(
        `invoke greet：${result.greeting ?? result.who} · Date=${dateOk ? "ok " + result.at.toISOString() : "FAIL"} · langs=${(result.langs ?? []).join("+")}`,
      );
      await window.tempo.session.push({ who: whoInput.value });
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

      const result = await window.ipcRenderer.invoke("sca-probe", outgoing);
      appendLog(`invoke sca-probe ok=${result?.ok}`);
      for (const line of result?.checks ?? []) appendLog(`  rt: ${line}`);
      for (const line of describeScaValue(result?.outgoing, "invoke result.outgoing")) {
        appendLog(line);
      }
      appendLog(
        allScaOk(result?.outgoing) ? "✓ invoke/handle SCA 通过" : "✗ invoke/handle SCA 失败",
      );

      // Fire-and-forget send; pong arrives via ipcRenderer.on("sca-pong").
      window.ipcRenderer.send("sca-ping", buildScaFixture("ui-ping"));
      appendLog("send sca-ping 已发出（等待 sca-pong）");
    } catch (error) {
      appendLog(`SCA 探测失败：${error.message ?? error}`);
    } finally {
      scaButton.disabled = false;
    }
  });

  permissionsButton.addEventListener("click", async () => {
    permissionsButton.disabled = true;
    permissionPolicyEl.textContent = "Manifest：检查中";
    setPermissionResult("host-ui", "检查中", "blocked");
    setPermissionResult("host-runtime", "检查中", "blocked");
    for (const name of permissionNames) setPermissionResult(name, "检查中", "blocked");
    try {
      const uiHostOk = await runUiFileProbe();
      setPermissionResult("host-ui", uiHostOk ? "允许（读写成功）" : "结果不一致", uiHostOk ? "allowed" : "error");

      const result = await window.ipcRenderer.invoke("permission-probe");
      permissionPolicyEl.textContent = `Manifest：${JSON.stringify(result.declared ?? {})}`;
      setPermissionResult(
        "host-runtime",
        result.host?.ok ? "允许（完整操作成功）" : "操作失败",
        result.host?.ok ? "allowed" : "error",
      );

      for (const name of permissionNames) {
        if (name === "read" || name === "write") continue;
        renderQueriedPermission(name, result.permissions?.[name] ?? "unknown");
      }

      for (const name of ["read", "write"]) {
        const direct = result.direct?.[name];
        if (direct?.allowed) {
          setPermissionResult(name, "允许（实际调用成功）", "allowed");
        } else {
          const state = result.permissions?.[name] ?? "unknown";
          const error = direct?.error ?? "Error";
          setPermissionResult(name, `已阻止（${state} / ${error}）`, "blocked");
        }
      }
      appendLog(
        `权限检查：Host UI=${uiHostOk ? "ok" : "FAIL"} · Host Runtime=${result.host?.ok ? "ok" : "FAIL"} · Deno=${JSON.stringify(result.permissions)}`,
      );
    } catch (error) {
      permissionPolicyEl.textContent = "Manifest：检查失败";
      setPermissionResult("host-runtime", error.message ?? String(error), "error");
      appendLog(`权限检查失败：${error.message ?? error}`);
    } finally {
      permissionsButton.disabled = false;
    }
  });
