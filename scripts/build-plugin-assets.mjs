import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const sourceRoot = path.join(root, "templates", "plugins");
const outputRoot = path.join(root, "docs", "public", "plugin-assets");
const catalogPath = path.join(outputRoot, "catalog.json");
const schemaSource = path.join(root, "docs", "schemas", "plugin-manifest.schema.json");
const kinds = ["ui", "hybrid", "headless"];
const sharedBridgeFiles = ["bridge-client.js", "structured-clone.js"];

const releaseConfig = JSON.parse(
  await readFile(path.join(sourceRoot, "release.json"), "utf8"),
);
const { version, minPluginApi, publicBaseUrl } = releaseConfig;

for (const [name, value] of Object.entries({ version, minPluginApi, publicBaseUrl })) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`templates/plugins/release.json 缺少 ${name}`);
  }
}

const normalizedBaseUrl = publicBaseUrl.endsWith("/")
  ? publicBaseUrl
  : `${publicBaseUrl}/`;
const releaseRelativeRoot = `releases/${version}`;
const releaseOutputRoot = path.join(outputRoot, "releases", version);
const replaceExistingRelease = process.env.TEMPO_REPLACE_PLUGIN_RELEASE === "1";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function toPosix(value) {
  return value.split(path.sep).join("/");
}

function shouldRender(bytes) {
  const text = bytes.toString("utf8");
  return ["__PLUGIN_ID__", "__PLUGIN_NAME__", "__PACKAGE_NAME__", "__MANIFEST_SCHEMA_URL__"]
    .some((placeholder) => text.includes(placeholder));
}

function assertDeclarationHasNoExplicitAny(relativePath, bytes) {
  if (!toPosix(relativePath).endsWith(".d.ts")) return;
  if (/\bany\b/.test(bytes.toString("utf8"))) {
    throw new Error(`模板类型声明不允许使用 any: ${toPosix(relativePath)}`);
  }
}

async function walkFiles(directory, base = directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    if (entry.name === "dist" || entry.name === "node_modules") continue;
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await walkFiles(absolute, base)));
    else if (entry.isFile()) files.push(path.relative(base, absolute));
  }
  return files;
}

async function describeFile(source, relativePath, url) {
  const bytes = await readFile(source);
  assertDeclarationHasNoExplicitAny(relativePath, bytes);
  return {
    path: toPosix(relativePath),
    url,
    sha256: sha256(bytes),
    size: bytes.length,
    render: shouldRender(bytes),
  };
}

await mkdir(outputRoot, { recursive: true });

let catalog = { catalogVersion: 1, releases: [] };
try {
  catalog = JSON.parse(await readFile(catalogPath, "utf8"));
} catch (error) {
  if (error?.code !== "ENOENT") throw error;
}
if (catalog.catalogVersion !== 1 || !Array.isArray(catalog.releases)) {
  throw new Error("docs/public/plugin-assets/catalog.json 格式无效");
}

const schema = JSON.parse(await readFile(schemaSource, "utf8"));
const schemaUrl = new URL(`${releaseRelativeRoot}/plugin-manifest.schema.json`, normalizedBaseUrl).href;
schema.$id = schemaUrl;
const schemaBytes = Buffer.from(`${JSON.stringify(schema, null, 2)}\n`, "utf8");

const nextRelease = {
  version,
  minPluginApi,
  manifestSchema: {
    url: `${releaseRelativeRoot}/plugin-manifest.schema.json`,
    sha256: sha256(schemaBytes),
    size: schemaBytes.length,
  },
  templates: {},
};

const stagedFiles = [];
for (const kind of kinds) {
  const kindSource = path.join(sourceRoot, kind);
  const relativeFiles = await walkFiles(kindSource);
  const descriptors = [];
  for (const relativePath of relativeFiles) {
    const posixPath = toPosix(relativePath);
    const source = path.join(kindSource, relativePath);
    const target = path.join(releaseOutputRoot, kind, relativePath);
    const url = `${releaseRelativeRoot}/${kind}/${posixPath}`;
    stagedFiles.push({ source, target, relativePath, url, descriptors });
  }
  if (kind === "ui" || kind === "hybrid") {
    for (const fileName of sharedBridgeFiles) {
      const relativePath = path.join(".tempo", fileName);
      stagedFiles.push({
        source: path.join(root, "plugin-ui", fileName),
        target: path.join(releaseOutputRoot, kind, relativePath),
        relativePath,
        url: `${releaseRelativeRoot}/${kind}/.tempo/${fileName}`,
        descriptors,
      });
    }
  }
  nextRelease.templates[kind] = { files: descriptors };
}

for (const file of stagedFiles) {
  file.descriptors.push(
    await describeFile(file.source, file.relativePath, file.url),
  );
}
for (const template of Object.values(nextRelease.templates)) {
  template.files.sort((left, right) => left.path.localeCompare(right.path));
}

const existingIndex = catalog.releases.findIndex((release) => release.version === version);
const existing = existingIndex >= 0 ? catalog.releases[existingIndex] : undefined;
if (
  existing &&
  JSON.stringify(existing) !== JSON.stringify(nextRelease) &&
  !replaceExistingRelease
) {
  throw new Error(`模板 ${version} 已发布且内容发生变化，请先提升 templates/plugins/release.json 的 version`);
}

await rm(releaseOutputRoot, { recursive: true, force: true });
await mkdir(releaseOutputRoot, { recursive: true });
for (const file of stagedFiles) {
  await mkdir(path.dirname(file.target), { recursive: true });
  await cp(file.source, file.target);
}
await writeFile(path.join(releaseOutputRoot, "plugin-manifest.schema.json"), schemaBytes);
if (existingIndex >= 0) catalog.releases[existingIndex] = nextRelease;
else catalog.releases.push(nextRelease);
catalog.releases.sort((left, right) => left.version.localeCompare(right.version));
await writeFile(catalogPath, `${JSON.stringify(catalog, null, 2)}\n`);

console.log(`Plugin templates ${version} prepared in docs/public/plugin-assets`);
