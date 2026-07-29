import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Load gitignored `.env` without overriding variables already set in the shell. */
function loadDotEnv(filePath) {
  if (!existsSync(filePath)) return;
  for (const rawLine of readFileSync(filePath, "utf8").split("\n")) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue;
    const key = line.slice(0, eq).trim();
    let value = line.slice(eq + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    if (process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

loadDotEnv(resolve(root, ".env"));

const args = ["tauri", "build", ...process.argv.slice(2)];

if (!process.env.APPLE_SIGNING_IDENTITY?.trim()) {
  console.warn(
    "[build] APPLE_SIGNING_IDENTITY 未设置，macOS 包将使用 adhoc 签名（系统通知不可用）。",
  );
  console.warn(
    "[build] 本地可复制 .env.example 为 .env 并填入证书，或 export APPLE_SIGNING_IDENTITY=...",
  );
} else {
  console.info(`[build] APPLE_SIGNING_IDENTITY=${process.env.APPLE_SIGNING_IDENTITY}`);
}

if (!process.env.TAURI_SIGNING_PRIVATE_KEY?.trim()) {
  console.warn(
    "[build] TAURI_SIGNING_PRIVATE_KEY 未设置，将跳过签名更新包（createUpdaterArtifacts=false）。",
  );
  console.warn(
    "[build] 发布版请在 CI 中配置密钥，或本地导出 TAURI_SIGNING_PRIVATE_KEY 后再打包。",
  );
  args.push("--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
}

const result = spawnSync("npx", args, {
  cwd: root,
  stdio: "inherit",
  env: process.env,
});

process.exit(result.status ?? 1);
