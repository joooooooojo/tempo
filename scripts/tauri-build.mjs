import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const core = resolve(root, "core");

function run(args) {
  const result = spawnSync(process.execPath, args, { cwd: core, stdio: "inherit" });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run([resolve(root, "scripts/sync-app-version.mjs")]);
run([resolve(core, "node_modules/typescript/bin/tsc")]);
run([resolve(core, "node_modules/vite/bin/vite.js"), "build"]);
