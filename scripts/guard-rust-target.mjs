import { spawnSync } from "node:child_process";
import { readdirSync, statfsSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const GIB = 1024n ** 3n;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = join(root, "src-tauri", "Cargo.toml");
const targetDir = join(root, "src-tauri", "target");
const developmentDir = join(targetDir, "debug");
const reportOnly = process.argv.includes("--report");

function positiveNumberFromEnv(name, fallback) {
  const raw = process.env[name]?.trim();
  if (!raw) return fallback;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    console.error(`[rust-cache] ${name} must be a positive number, received: ${raw}`);
    process.exit(2);
  }
  return parsed;
}

const maxTargetGiB = positiveNumberFromEnv("TEMPO_RUST_TARGET_MAX_GIB", 12);
const minFreeGiB = positiveNumberFromEnv("TEMPO_MIN_FREE_DISK_GIB", 10);

function measureDirectory(path) {
  const pending = [path];
  const hardLinks = new Set();
  let bytes = 0n;
  let files = 0;

  while (pending.length > 0) {
    const current = pending.pop();
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") continue;
      throw error;
    }

    for (const entry of entries) {
      const entryPath = join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(entryPath);
        continue;
      }
      if (!entry.isFile()) continue;

      const stats = statSync(entryPath, { bigint: true });
      if (stats.nlink > 1n) {
        const identity = `${stats.dev}:${stats.ino}`;
        if (hardLinks.has(identity)) continue;
        hardLinks.add(identity);
      }
      bytes += stats.size;
      files += 1;
    }
  }

  return { bytes, files };
}

function freeDiskBytes(path) {
  const stats = statfsSync(path, { bigint: true });
  return stats.bavail * stats.bsize;
}

function formatGiB(bytes) {
  return (Number(bytes) / Number(GIB)).toFixed(2);
}

const usage = measureDirectory(developmentDir);
const diskProbe = statSync(targetDir, { throwIfNoEntry: false }) ? targetDir : root;
const freeBytes = freeDiskBytes(diskProbe);
const targetLimit = BigInt(Math.floor(maxTargetGiB * Number(GIB)));
const freeLimit = BigInt(Math.floor(minFreeGiB * Number(GIB)));

console.log(
  `[rust-cache] target/debug=${formatGiB(usage.bytes)} GiB (${usage.files} files), ` +
    `free=${formatGiB(freeBytes)} GiB`
);

if (reportOnly || (usage.bytes <= targetLimit && freeBytes >= freeLimit)) {
  process.exit(0);
}

const reasons = [];
if (usage.bytes > targetLimit) reasons.push(`target/debug exceeds ${maxTargetGiB} GiB`);
if (freeBytes < freeLimit) reasons.push(`free disk is below ${minFreeGiB} GiB`);
console.warn(`[rust-cache] ${reasons.join(" and ")}; cleaning generated Rust artifacts...`);

const result = spawnSync(
  "cargo",
  [
    "clean",
    "--manifest-path",
    manifestPath,
    "--profile",
    "dev",
    "--target-dir",
    targetDir,
  ],
  { cwd: root, stdio: "inherit" }
);
if (result.error) {
  console.error(`[rust-cache] cargo clean failed: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
