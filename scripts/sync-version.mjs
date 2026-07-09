import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJsonPath = resolve(root, "package.json");
const cargoTomlPath = resolve(root, "src-tauri", "Cargo.toml");
const tauriConfigPath = resolve(root, "src-tauri", "tauri.conf.json");

const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
const version = packageJson.version;

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`Invalid package.json version: ${version}`);
}

await updateJson(tauriConfigPath, (json) => {
  json.version = version;
});

await updateText(cargoTomlPath, (text) =>
  replaceRequired(
    text,
    /(^\[package\][\s\S]*?^version\s*=\s*")[^"]+(")/m,
    `$1${version}$2`,
    "Cargo.toml [package].version"
  )
);

console.log(`Synced Tempo version to ${version}`);

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
