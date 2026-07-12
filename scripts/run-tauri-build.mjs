import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = ["tauri", "build", ...process.argv.slice(2)];

if (!process.env.TAURI_SIGNING_PRIVATE_KEY?.trim()) {
  console.warn(
    "[build] TAURI_SIGNING_PRIVATE_KEY 未设置，将跳过签名更新包（createUpdaterArtifacts=false）。"
  );
  console.warn(
    "[build] 发布版请在 CI 中配置密钥，或本地导出 TAURI_SIGNING_PRIVATE_KEY 后再打包。"
  );
  args.push("--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
}

const result = spawnSync("npx", args, {
  cwd: root,
  stdio: "inherit",
  env: process.env,
});

process.exit(result.status ?? 1);
