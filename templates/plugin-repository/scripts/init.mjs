#!/usr/bin/env node
/**
 * Fill in repository identity after copying this template.
 *
 *   node scripts/init.mjs
 *   node scripts/init.mjs --remove-example
 */

import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const INDEX_PATH = path.join(ROOT, "tempo-plugin-repository.json");
const PLUGIN_ID = /^(?!(?:builtin|tempo)(?:\.|$))[a-z0-9]+(?:\.[a-z0-9-]+)+$/;
const REPOSITORY_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const EXAMPLE_ID = "com.example.welcome";
const EXAMPLE_DIR = path.join(ROOT, "plugins", EXAMPLE_ID);

function usage(exitCode = 0) {
  console.log(`Usage:
  node scripts/init.mjs [options]

Options:
  --id             可选。默认自动生成 UUID
  --description    可选说明
  --homepage       可选 HTTP(S) 主页
  --remove-example 删除示例插件 com.example.welcome
`);
  process.exit(exitCode);
}

function argValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return undefined;
  return process.argv[index + 1];
}

function isRepositoryId(value) {
  return typeof value === "string" && (REPOSITORY_UUID.test(value) || (PLUGIN_ID.test(value) && value.length <= 128));
}

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  usage(0);
}

const id = argValue("--id") ?? randomUUID();
const description = argValue("--description");
const homepage = argValue("--homepage");
const removeExample = process.argv.includes("--remove-example");

if (!isRepositoryId(id)) {
  console.error(`无效的仓库 ID: ${id}
ID 必须是小写 UUID，例如 7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d。`);
  process.exit(1);
}

if (description != null && [...description].length > 1024) {
  console.error("description 不能超过 1024 个字符。");
  process.exit(1);
}

if (homepage != null) {
  let parsed;
  try {
    parsed = new URL(homepage);
  } catch {
    parsed = null;
  }
  if (!parsed || (parsed.protocol !== "http:" && parsed.protocol !== "https:") || homepage.length > 2048) {
    console.error("homepage 必须是不超过 2048 字符的 HTTP(S) URL。");
    process.exit(1);
  }
}

const index = JSON.parse(fs.readFileSync(INDEX_PATH, "utf8"));
index.id = id;
delete index.name;
if (description != null) index.description = description;
if (homepage != null) index.homepage = homepage;

if (removeExample) {
  index.plugins = (index.plugins ?? []).filter((plugin) => plugin.id !== EXAMPLE_ID);
  fs.rmSync(EXAMPLE_DIR, { recursive: true, force: true });
}

fs.writeFileSync(INDEX_PATH, `${JSON.stringify(index, null, 2)}\n`);
console.log(`Updated ${path.relative(ROOT, INDEX_PATH)}`);
console.log(`  id: ${index.id}`);
if (removeExample) {
  console.log("Removed example plugin com.example.welcome");
}
console.log("\nNext: commit, push, then add this Git URL in Tempo → Settings → Plugins → 插件仓库.");
