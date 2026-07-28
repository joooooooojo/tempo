import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJsonPath = resolve(root, "package.json");
const cargoTomlPath = resolve(root, "src-tauri", "Cargo.toml");
const tauriConfigPath = resolve(root, "src-tauri", "tauri.conf.json");
const pluginSdkPackageJsonPath = resolve(root, "packages", "plugin-sdk", "package.json");

const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
const version = packageJson.version;

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`Invalid package.json version: ${version}`);
}

await updateJson(tauriConfigPath, (json) => {
  json.version = version;
});

await updateJson(pluginSdkPackageJsonPath, (json) => {
  json.version = version;
});

const pluginSdkLockPath = resolve(root, "packages", "plugin-sdk", "package-lock.json");
try {
  await updateJson(pluginSdkLockPath, (json) => {
    json.version = version;
    if (json.packages?.[""]) {
      json.packages[""].version = version;
    }
  });
} catch (error) {
  if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
    // Optional lockfile (repo may use root pnpm lock only).
  } else {
    throw error;
  }
}

await updateText(cargoTomlPath, (text) =>
  replaceRequired(
    text,
    /(^\[package\][\s\S]*?^version\s*=\s*")[^"]+(")/m,
    `$1${version}$2`,
    "Cargo.toml [package].version"
  )
);

console.log(`Synced Tempo version to ${version} (app + @tempo/plugin-sdk)`);

async function updateJson(path, mutate) {
  const json = JSON.parse(await readFile(path, "utf8"));
  mutate(json);
  await writeFile(path, `${JSON.stringify(json, null, 2)}\n`);
}

async function updateText(path, mutate) {
  const text = await readFile(path, "utf8");
  const next = mutate(text);
  await writeFile(path, next);
}

function replaceRequired(text, pattern, replacement, label) {
  if (!pattern.test(text)) {
    throw new Error(`Could not update ${label}`);
  }

  const next = text.replace(pattern, replacement);
  return next;
}
