#!/usr/bin/env node
/**
 * Validate a Tempo plugin repository:
 * - tempo-plugin-repository.json structure, IDs, and paths
 * - each indexed plugin's committed dist/ package
 *
 * Usage: node scripts/validate.mjs
 */

import fs from "node:fs";

function validatePermissions(policy = {}, pluginId) {
  const invalid = message => fail(`${pluginId}: permissions ${message}`);
  if (!policy || typeof policy !== "object" || Array.isArray(policy)) return invalid("必须是对象");
  const known = ["read", "write", "net", "env", "host"];
  for (const key of Object.keys(policy)) if (!known.includes(key)) invalid(`不支持 ${key}`);
  for (const key of ["read", "write", "net", "env"]) {
    const scopes = policy[key] ?? [];
    if (!Array.isArray(scopes) || scopes.some(s => typeof s !== "string")) { invalid(`${key} 必须是字符串数组`); continue; }
    for (const scope of scopes) {
      if ((key === "read" || key === "write") && scope !== "$DATA") invalid(`${key} 只接受 $DATA`);
      if (key === "env" && (!/^[A-Z0-9_]+$/.test(scope) || /^(DENO_|NODE_|TEMPO_|LD_|DYLD_)/.test(scope) || scope === "PATH")) invalid(`保留或无效环境变量 ${scope}`);
      if (key === "net") {
        try {
          const url = new URL(`http://${scope}`);
          const port = scope.slice(scope.lastIndexOf(":") + 1);
          if (!/^[0-9]+$/.test(port) || Number(port) < 1 || Number(port) > 65535 || !url.hostname || url.username || url.password || url.pathname !== "/" || url.search || url.hash || /[\s,/*\\@%]/.test(scope)) throw new Error();
        } catch { invalid(`net 需要明确的 host:port: ${scope}`); }
      }
    }
  }
  const host = policy.host ?? {};
  if (!host || typeof host !== "object" || Array.isArray(host)) return invalid("host 必须是对象");
  for (const key of Object.keys(host)) if (!["notify", "externalOpen", "openApps"].includes(key)) invalid(`host 不支持 ${key}`);
  for (const key of ["notify", "externalOpen"]) if (key in host && typeof host[key] !== "boolean") invalid(`host.${key} 必须是布尔值`);
  if ("openApps" in host && (!Array.isArray(host.openApps) || host.openApps.some(id => typeof id !== "string" || !id || id.includes("*")))) invalid("host.openApps 需要精确 App ID 数组");
}
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const INDEX_FILE = "tempo-plugin-repository.json";
const INDEX_MAX_BYTES = 2 * 1024 * 1024;
const MAX_PLUGINS = 5000;
const MAX_DIST_FILES = 10_000;
const MAX_DIST_FILE_BYTES = 200 * 1024 * 1024;
const MAX_DIST_BYTES = 500 * 1024 * 1024;
const INDEX_KEYS = new Set(["$schema", "schemaVersion", "id", "name", "description", "homepage", "plugins"]);
const ENTRY_KEYS = new Set(["id", "path"]);
const PLUGIN_ID = /^(?!(?:builtin|tempo)(?:\.|$))[a-z0-9]+(?:\.[a-z0-9-]+)+$/;
const REPOSITORY_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const LFS_POINTER = /^version https:\/\/git-lfs\.github\.com\/spec\/v1\b/;

const errors = [];

function fail(message) {
  errors.push(message);
}

function isPluginId(value) {
  return typeof value === "string" && value.length >= 1 && value.length <= 128 && PLUGIN_ID.test(value);
}

function isRepositoryId(value) {
  return typeof value === "string" && (REPOSITORY_UUID.test(value) || isPluginId(value));
}

function charCount(value) {
  return [...value].length;
}

function assertSafePath(value, label) {
  if (typeof value !== "string" || !value.trim()) {
    fail(`${label} 不能为空`);
    return null;
  }
  const pathValue = value.trim();
  if (
    pathValue.startsWith("/") ||
    pathValue.includes("\\") ||
    pathValue.includes("\0") ||
    pathValue.includes(":") ||
    pathValue.split("/").some((part) => !part || part === "." || part === ".." || part === ".git")
  ) {
    fail(`无效的${label}: ${value}`);
    return null;
  }
  if (pathValue.normalize("NFC") !== pathValue) {
    fail(`${label} 必须使用 Unicode NFC: ${value}`);
    return null;
  }
  return pathValue;
}

function parseJsonRejectDuplicates(text, label) {
  try {
    const value = JSON.parse(text);
    rejectDuplicateKeys(text, label);
    return value;
  } catch (error) {
    fail(`${label}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

function rejectDuplicateKeys(source, label) {
  let i = 0;
  const skipWs = () => {
    while (i < source.length && /\s/.test(source[i])) i += 1;
  };
  const parseString = () => {
    if (source[i] !== "\"") throw new Error("expected string");
    i += 1;
    let out = "";
    while (i < source.length) {
      const current = source[i];
      i += 1;
      if (current === "\"") return out;
      if (current === "\\") {
        const escaped = source[i];
        i += 1;
        if (escaped === "u") {
          i += 4;
          out += "x";
        } else {
          out += escaped;
        }
        continue;
      }
      out += current;
    }
    throw new Error("unterminated string");
  };
  const parseValue = () => {
    skipWs();
    const current = source[i];
    if (current === "\"") {
      parseString();
      return;
    }
    if (current === "{") {
      parseObject();
      return;
    }
    if (current === "[") {
      parseArray();
      return;
    }
    if (current === "t") {
      i += 4;
      return;
    }
    if (current === "f") {
      i += 5;
      return;
    }
    if (current === "n") {
      i += 4;
      return;
    }
    if (current === "-" || (current >= "0" && current <= "9")) {
      while (i < source.length && /[0-9eE+.\-]/.test(source[i])) i += 1;
      return;
    }
    throw new Error(`unexpected token ${JSON.stringify(current)}`);
  };
  const parseObject = () => {
    i += 1;
    const keys = new Set();
    skipWs();
    if (source[i] === "}") {
      i += 1;
      return;
    }
    while (true) {
      skipWs();
      const key = parseString();
      if (keys.has(key)) {
        throw new Error(`${label} 重复对象键: ${key}`);
      }
      keys.add(key);
      skipWs();
      if (source[i] !== ":") throw new Error("expected :");
      i += 1;
      parseValue();
      skipWs();
      if (source[i] === ",") {
        i += 1;
        continue;
      }
      if (source[i] === "}") {
        i += 1;
        return;
      }
      throw new Error("expected , or }");
    }
  };
  const parseArray = () => {
    i += 1;
    skipWs();
    if (source[i] === "]") {
      i += 1;
      return;
    }
    while (true) {
      parseValue();
      skipWs();
      if (source[i] === ",") {
        i += 1;
        continue;
      }
      if (source[i] === "]") {
        i += 1;
        return;
      }
      throw new Error("expected , or ]");
    }
  };
  parseValue();
}

function walkFiles(dir, relative = "") {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const rel = relative ? `${relative}/${entry.name}` : entry.name;
    const full = path.join(dir, entry.name);
    if (entry.isSymbolicLink()) {
      fail(`dist 不能包含符号链接: ${rel}`);
      continue;
    }
    if (entry.isDirectory()) {
      files.push(...walkFiles(full, rel));
      continue;
    }
    if (entry.isFile()) {
      files.push({ rel, full });
    }
  }
  return files;
}

function validateDist(pluginId, pluginRoot) {
  const dist = path.join(ROOT, pluginRoot, "dist");
  if (!fs.existsSync(dist) || !fs.statSync(dist).isDirectory()) {
    fail(`${pluginId}: 缺少 ${pluginRoot}/dist`);
    return;
  }

  const files = walkFiles(dist);
  if (files.length > MAX_DIST_FILES) {
    fail(`${pluginId}: dist 文件数超过 ${MAX_DIST_FILES}`);
  }

  let totalBytes = 0;
  const folded = new Set();
  for (const file of files) {
    const lower = file.rel.toLowerCase();
    if (folded.has(lower)) {
      fail(`${pluginId}: dist 路径大小写冲突: ${file.rel}`);
    }
    folded.add(lower);
    const size = fs.statSync(file.full).size;
    totalBytes += size;
    if (size > MAX_DIST_FILE_BYTES) {
      fail(`${pluginId}: 单文件超过 200 MiB: ${file.rel}`);
    }
    if (size < 1024) {
      const head = fs.readFileSync(file.full, "utf8");
      if (LFS_POINTER.test(head)) {
        fail(`${pluginId}: dist 含 Git LFS pointer，必须提交真实文件: ${file.rel}`);
      }
    }
  }
  if (totalBytes > MAX_DIST_BYTES) {
    fail(`${pluginId}: dist 总大小超过 500 MiB`);
  }

  const manifestPath = path.join(dist, "manifest.json");
  if (!fs.existsSync(manifestPath)) {
    fail(`${pluginId}: dist/manifest.json 不存在`);
    return;
  }
  const raw = fs.readFileSync(manifestPath, "utf8");
  const manifest = parseJsonRejectDuplicates(raw, `${pluginId} dist/manifest.json`);
  if (!manifest || typeof manifest !== "object") return;

  if (manifest.manifestVersion !== 2) {
    fail(`${pluginId}: manifestVersion 必须是 2`);
  }
  validatePermissions(manifest.permissions, pluginId);
  if (manifest.id !== pluginId) {
    fail(`${pluginId}: 索引 ID 与 dist/manifest.json 的 id (${manifest.id}) 不一致`);
  }
  if (typeof manifest.name !== "string" || !manifest.name.trim()) {
    fail(`${pluginId}: Manifest name 无效`);
  }
  if (typeof manifest.version !== "string" || !manifest.version.trim()) {
    fail(`${pluginId}: Manifest version 无效`);
  }
  if (!manifest.engines || typeof manifest.engines.tempo !== "string" || typeof manifest.engines.pluginApi !== "string") {
    fail(`${pluginId}: engines.tempo 和 engines.pluginApi 必填`);
  }

  const apps = manifest.contributes?.apps ?? [];
  const hasUi = Array.isArray(apps) && apps.length > 0;
  if (hasUi && !fs.existsSync(path.join(dist, "index.html"))) {
    fail(`${pluginId}: UI 插件需要 dist/index.html`);
  }
  if (typeof manifest.main === "string") {
    if (!fs.existsSync(path.join(dist, manifest.main))) {
      fail(`${pluginId}: main 入口不存在: ${manifest.main}`);
    }
  } else if (!hasUi) {
    fail(`${pluginId}: Headless 插件需要 main.js 或 main.mjs`);
  }
  for (const app of apps) {
    if (app?.entry && !fs.existsSync(path.join(dist, app.entry))) {
      fail(`${pluginId}: app 入口不存在: ${app.entry}`);
    }
  }

  console.log(`OK  ${pluginId}  ${pluginRoot}  v${manifest.version}`);
}

function validateIndex(index) {
  if (!index || typeof index !== "object" || Array.isArray(index)) {
    fail("仓库索引必须是对象");
    return;
  }
  for (const key of Object.keys(index)) {
    if (!INDEX_KEYS.has(key)) fail(`仓库索引含未知字段: ${key}`);
  }
  if (index.schemaVersion !== 1) fail(`不支持仓库索引版本 ${index.schemaVersion}`);
  if (!isRepositoryId(index.id)) fail(`仓库索引 id 无效: ${index.id}`);
  if (index.name != null && (typeof index.name !== "string" || !index.name.trim() || charCount(index.name) > 128)) {
    fail("仓库索引 name 无效");
  }
  if (index.description != null && (typeof index.description !== "string" || charCount(index.description) > 1024)) {
    fail("仓库索引 description 超过 1024 个字符");
  }
  if (index.homepage != null) {
    if (typeof index.homepage !== "string" || index.homepage.length > 2048) {
      fail("仓库索引 homepage 过长");
    } else {
      try {
        const url = new URL(index.homepage);
        if (url.protocol !== "http:" && url.protocol !== "https:") {
          fail("仓库索引 homepage 必须使用 HTTP(S)");
        }
      } catch {
        fail(`仓库索引 homepage 无效: ${index.homepage}`);
      }
    }
  }
  if (!Array.isArray(index.plugins)) {
    fail("仓库索引 plugins 必须是数组");
    return;
  }
  if (index.plugins.length > MAX_PLUGINS) fail("仓库索引包含过多插件");

  const ids = new Set();
  const paths = new Set();
  const normalized = [];
  for (const plugin of index.plugins) {
    if (!plugin || typeof plugin !== "object") {
      fail("插件索引项必须是对象");
      continue;
    }
    for (const key of Object.keys(plugin)) {
      if (!ENTRY_KEYS.has(key)) fail(`插件索引项含未知字段: ${key}`);
    }
    if (!isPluginId(plugin.id) || ids.has(plugin.id.toLowerCase())) {
      fail(`仓库索引包含无效或重复插件 ID: ${plugin.id}`);
    } else {
      ids.add(plugin.id.toLowerCase());
    }
    const pluginPath = assertSafePath(plugin.path, "插件目录");
    if (!pluginPath) continue;
    const folded = pluginPath.toLowerCase();
    if (paths.has(folded)) {
      fail(`仓库索引包含重复插件路径: ${plugin.path}`);
      continue;
    }
    paths.add(folded);
    if (normalized.some((previous) => folded.startsWith(`${previous}/`) || previous.startsWith(`${folded}/`))) {
      fail(`仓库索引中的插件目录不能互相嵌套: ${plugin.path}`);
    }
    normalized.push(folded);
    validateDist(plugin.id, pluginPath);
  }
}

const indexPath = path.join(ROOT, INDEX_FILE);
if (!fs.existsSync(indexPath)) {
  fail(`缺少 ${INDEX_FILE}`);
} else {
  const stat = fs.statSync(indexPath);
  if (stat.size > INDEX_MAX_BYTES) fail(`${INDEX_FILE} 超过 2 MiB`);
  const text = fs.readFileSync(indexPath, "utf8");
  const index = parseJsonRejectDuplicates(text, INDEX_FILE);
  if (index) validateIndex(index);
}

if (errors.length > 0) {
  for (const error of errors) {
    console.error(`ERROR  ${error}`);
  }
  console.error(`\nFailed with ${errors.length} error(s).`);
  process.exit(1);
}

console.log("\nRepository index is valid.");
