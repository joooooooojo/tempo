import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = resolve(root, "src-tauri");
const source = resolve(tauriDir, "app-icon.png");
const padded = resolve(tauriDir, ".app-icon-macos.png");
const iconsDir = resolve(tauriDir, "icons");
const macTmpDir = resolve(tauriDir, ".icons-macos-tmp");

// macOS HIG: 824×824 artwork centered in a 1024×1024 canvas (~100px transparent gutter).
// Windows/Linux use the full-bleed source so taskbar/start-menu icons have no extra margin.
// https://v2.tauri.app/develop/icons/
const MACOS_CONTENT = 824;
const MACOS_CANVAS = 1024;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...options });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function createMacPaddedSource() {
  run("magick", [
    source,
    "-resize",
    `${MACOS_CONTENT}x${MACOS_CONTENT}`,
    "-background",
    "none",
    "-gravity",
    "center",
    "-extent",
    `${MACOS_CANVAS}x${MACOS_CANVAS}`,
    padded,
  ]);
}

rmSync(iconsDir, { recursive: true, force: true });
mkdirSync(iconsDir, { recursive: true });

// Windows / Linux / dev: full-bleed artwork (icon.ico + PNG set).
run("npx", ["tauri", "icon", source, "-o", "icons"], { cwd: tauriDir });

// macOS only: replace icon.icns with the padded safe-area variant.
createMacPaddedSource();
rmSync(macTmpDir, { recursive: true, force: true });
mkdirSync(macTmpDir, { recursive: true });
run("npx", ["tauri", "icon", padded, "-o", macTmpDir], { cwd: tauriDir });
cpSync(resolve(macTmpDir, "icon.icns"), resolve(iconsDir, "icon.icns"));

rmSync(padded, { force: true });
rmSync(macTmpDir, { recursive: true, force: true });

for (const extra of ["ios", "android", "AppIcon.iconset"]) {
  rmSync(resolve(iconsDir, extra), { recursive: true, force: true });
}

cpSync(resolve(iconsDir, "128x128.png"), resolve(root, "public/favicon.png"));

console.log(
  "Icons generated: Windows/Linux from full app-icon.png; macOS icon.icns with safe-area padding."
);
