import { Buffer } from "node:buffer";
import { readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { scaEncode, scaDecodeArgs, scaEncodeArgs } from "../../core/plugin-runtime/structured-clone.mjs";

onMounted(() => {
  tempo.mcpTools.register("echo", async args => args);
  ipcMain.handle("echo", async (_event, value) => value);
  tempo.commands.register("probe", async (params) => {
    const original = { date: new Date("2026-01-01T00:00:00Z"), bytes: new Uint8Array(Buffer.from("ok")) };
    original.self = original;
    const [copy] = scaDecodeArgs(scaEncodeArgs([original]));
    const result = {
      cycle: copy.self === copy,
      date: copy.date.toISOString(),
      bytes: Array.from(copy.bytes),
      envelope: typeof scaEncode(original).$sca,
      runtime: tempo.runtime,
    };
    await tempo.files.mkdir("host", { recursive: true });
    await tempo.files.writeText("host/probe.txt", "host-api");
    result.hostFileText = await tempo.files.readText("host/probe.txt");
    await tempo.files.writeBytes("host/probe.bin", new Uint8Array([0, 1, 255]));
    result.hostFileBytes = Array.from(await tempo.files.readBytes("host/probe.bin"));
    if (typeof Deno !== "undefined") {
      try {
        await readdir(tempo.paths.data);
        result.deniedRead = false;
      } catch (error) {
        result.deniedRead = error.name === "NotCapable";
      }
      try { await writeFile(join(tempo.paths.data, "probe.txt"), "ok"); result.dataWrite = true; }
      catch (error) { if (error.name !== "NotCapable") throw error; result.dataWrite = false; }
      const denied = async (fn) => { try { await fn(); return false; } catch (error) { return error.name === "NotCapable"; } };
      result.deniedEnv = await denied(() => Deno.env.get("TEMPO_TEST_SECRET"));
      result.deniedRun = await denied(() => new Deno.Command(params.runExecutable, {args:["--version"]}).output());
      result.deniedNet = await denied(() => Deno.connect({hostname:"127.0.0.1",port:1}));
      result.deniedFfi = await denied(() => Deno.dlopen("missing-native-library", {}));
      for (const [key, specifier] of [["deniedRemoteImport", "https://example.com/probe.js"], ["deniedNpmImport", "npm:clsx"]]) {
        try { await import(/* @vite-ignore */ specifier); result[key] = false; } catch { result[key] = true; }
      }
    }
    return result;
  });
});
