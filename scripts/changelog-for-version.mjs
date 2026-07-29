#!/usr/bin/env node
/**
 * Extract a CHANGELOG.md section for GitHub Release body.
 *
 * Usage:
 *   node ./scripts/changelog-for-version.mjs [version]
 *   RELEASE_TAG=v2.0.1 node ./scripts/changelog-for-version.mjs
 *
 * Prints Markdown to stdout. Falls back to a short default if the section is missing.
 */
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const changelogPath = resolve(root, "CHANGELOG.md");

const rawVersion =
  process.argv[2] ||
  process.env.RELEASE_VERSION ||
  (process.env.RELEASE_TAG || "").replace(/^v/, "") ||
  JSON.parse(await readFile(resolve(root, "package.json"), "utf8")).version;

const version = String(rawVersion).replace(/^v/, "");
const changelog = await readFile(changelogPath, "utf8");

const heading = `## [${version}]`;
const start = changelog.indexOf(heading);
if (start === -1) {
  process.stdout.write(
    [
      `## Tempo v${version}`,
      "",
      "详见仓库 [`CHANGELOG.md`](./CHANGELOG.md)。",
      "",
      "在应用内打开「设置 → 关于」检查更新，或下载对应平台的安装包手动安装。",
      "",
    ].join("\n"),
  );
  process.exit(0);
}

let end = changelog.indexOf("\n## [", start + heading.length);
if (end === -1) end = changelog.length;
const section = changelog.slice(start, end).trim();

process.stdout.write(
  [
    `## Tempo v${version}`,
    "",
    section.replace(/^## \[[^\]]+\][^\n]*/, "").trim(),
    "",
    "---",
    "",
    "在应用内打开「设置 → 关于」检查更新，或下载对应平台的安装包手动安装。",
    "",
    `完整记录见 [\`CHANGELOG.md\`](https://github.com/${process.env.GITHUB_REPOSITORY || "owner/repo"}/blob/v${version}/CHANGELOG.md)。`,
    "",
  ].join("\n"),
);
