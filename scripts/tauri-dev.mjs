import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const core = resolve(root, "core");

const child = spawn(
  process.execPath,
  [resolve(core, "node_modules/vite/bin/vite.js"), "--host", "127.0.0.1"],
  { cwd: core, stdio: "inherit" }
);

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
