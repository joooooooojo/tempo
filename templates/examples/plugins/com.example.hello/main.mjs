const PRIVATE_LOG_PATH = "hello.log";
const PERMISSION_DEMO_DIRECTORY = "permissions-demo";

let logWriteQueue = Promise.resolve();

async function appendPrivateLog(line) {
  const write = logWriteQueue.then(async () => {
    const existing = (await tempo.files.stat(PRIVATE_LOG_PATH))
      ? await tempo.files.readText(PRIVATE_LOG_PATH)
      : "";
    await tempo.files.writeText(PRIVATE_LOG_PATH, `${existing}${line}`);
  });
  logWriteQueue = write.catch(() => {});
  return write;
}

async function removePrivateFileIfPresent(path) {
  if (await tempo.files.stat(path)) {
    await tempo.files.remove(path);
  }
}

async function attempt(operation) {
  try {
    await operation();
    return { allowed: true, error: null };
  } catch (error) {
    return {
      allowed: false,
      error: error && typeof error === "object" && "name" in error ? String(error.name) : "Error",
    };
  }
}

async function queryDenoPermissions(readPath, writePath, declaredPermissions) {
  const descriptors = {
    read: { name: "read", path: readPath },
    write: { name: "write", path: writePath },
    net: { name: "net", host: "example.com:443" },
    env: { name: "env", variable: "PERMISSION_DEMO_VALUE" },
    sys: { name: "sys", kind: "hostname" },
    run: { name: "run", command: "permission-demo-command" },
    ffi: { name: "ffi", path: readPath },
  };
  const entries = await Promise.all(
    Object.entries(descriptors).map(async ([name, descriptor]) => {
      try {
        const status = await Deno.permissions.query(descriptor);
        return [name, status.state];
      } catch (error) {
        const reason = error && typeof error === "object" && "name" in error
          ? String(error.name)
          : "unsupported";
        return [name, reason];
      }
    }),
  );
  return {
    ...Object.fromEntries(entries),
    // Restricted runtimes always use --no-remote; Deno reports the import query as granted
    // because Host IPC needs --allow-net, so derive the effective import policy here.
    import: declaredPermissions.includes("import") ? "granted" : "denied",
  };
}

async function readDeclaredPermissions() {
  const manifestUrl = new URL("./manifest.json", import.meta.url);
  const manifest = JSON.parse(await Deno.readTextFile(manifestUrl));
  return Array.isArray(manifest.permissions) ? manifest.permissions : [];
}

async function runPermissionProbe() {
  const directory = PERMISSION_DEMO_DIRECTORY;
  const hostTextPath = `${directory}/host.txt`;
  const hostBytesPath = `${directory}/host.bin`;
  const draftPath = `${directory}/draft.txt`;
  const renamedPath = `${directory}/renamed.txt`;

  await tempo.files.mkdir(directory, { recursive: true });
  await removePrivateFileIfPresent(renamedPath);
  await tempo.files.writeText(hostTextPath, "tempo.files works without Deno file permission");
  await tempo.files.writeBytes(hostBytesPath, new Uint8Array([0, 1, 2, 255]));
  await tempo.files.writeText(draftPath, "rename works");
  await tempo.files.rename(draftPath, renamedPath);

  const hostText = await tempo.files.readText(hostTextPath);
  const hostBytes = await tempo.files.readBytes(hostBytesPath);
  const hostStat = await tempo.files.stat(renamedPath);
  const hostEntries = await tempo.files.list(directory);
  const globalReadPath = Deno.execPath();
  const globalWritePath = `${tempo.paths.data}/../tempo-permission-demo-${Deno.pid}.txt`;
  const declared = await readDeclaredPermissions();
  const permissions = await queryDenoPermissions(globalReadPath, globalWritePath, declared);
  const directRead = await attempt(() => Deno.readFile(globalReadPath));
  const directWrite = await attempt(() =>
    Deno.writeTextFile(globalWritePath, "Deno global write succeeded"),
  );
  if (directWrite.allowed) {
    await Deno.remove(globalWritePath);
  }

  return {
    declared,
    permissions,
    host: {
      ok:
        hostText === "tempo.files works without Deno file permission" &&
        hostBytes.join(",") === "0,1,2,255" &&
        hostStat?.type === "file" &&
        hostEntries.some((entry) => entry.name === "renamed.txt"),
      text: hostText,
      bytes: Array.from(hostBytes),
      entries: hostEntries.map((entry) => entry.name),
      renamedType: hostStat?.type ?? null,
    },
    direct: {
      read: directRead,
      write: directWrite,
    },
  };
}

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

async function greet(params) {
  const settings = await tempo.settings.getAll();
  const { loud, defaultWho, theme, langs } = readPluginSettings(settings);
  const who =
    typeof params?.who === "string" && params.who.trim() ? params.who.trim() : defaultWho;
  const greeting = loud ? `HELLO, ${who.toUpperCase()}!` : `Hello, ${who}!`;
  const langLabel = langs.length > 0 ? langs.join("+") : "none";
  const at = new Date();
  const timestamp = at.toISOString();
  const line = `${greeting} [theme=${theme}; langs=${langLabel}] (${timestamp})\n`;

  await appendPrivateLog(line);

  await tempo.notify.show({
    title: loud ? "HELLO 示例插件" : "Hello 示例插件",
    body: `${greeting} · 主题 ${theme} · 语言 ${langLabel}`,
  });

  // Event payload includes a Date so UI can verify ipcMain.send -> ipcRenderer.on SCA.
  ipcMain.send("greeted", {
    who,
    at,
    loud,
    theme,
    langs,
  });

  return { who, at, timestamp, logPath: PRIVATE_LOG_PATH, loud, theme, langs, greeting };
}

onMounted(() => {
  ipcMain.handle("greet", async (_event, params) => greet(params ?? {}));
  ipcMain.handle("permission-probe", () => runPermissionProbe());

  // Dedicated SCA probe: UI -> Runtime invoke + Runtime -> UI send.
  ipcMain.handle("sca-probe", async (_event, incoming) => {
    console.log("[hello] sca-probe received", describeScaValue(incoming, "in").join(" | "));
    const outgoing = buildScaFixture(`echo:${incoming?.label ?? "?"}`);
    ipcMain.send("sca-echo", outgoing);
    return {
      ok: true,
      checks: describeScaValue(incoming, "in"),
      outgoing,
    };
  });

  // UI -> Runtime fire-and-forget send -> on.
  ipcMain.on("sca-ping", (_event, incoming) => {
    console.log("[hello] sca-ping received", describeScaValue(incoming, "ping").join(" | "));
    ipcMain.send("sca-pong", buildScaFixture(`pong:${incoming?.label ?? "?"}`));
  });

  // Declared Command for Actions only (UI cannot invoke Commands).
  tempo.commands.register("hello", async (params) => greet(params));

  // MCP Tools use their own registry and never route through Commands.
  tempo.mcpTools.register("say-hello", async (params) => greet(params));

  tempo.settings.subscribe((values) => {
    console.log("[hello] settings changed", values);
  });
});
