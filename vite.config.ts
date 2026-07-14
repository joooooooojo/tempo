import path from "path";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

declare const process: {
  env: Record<string, string | undefined>;
  platform: string;
};

const host = process.env.TAURI_DEV_HOST;
const targetPlatform = normalizeTargetPlatform(
  process.env.TAURI_ENV_PLATFORM ?? process.env.CARGO_CFG_TARGET_OS ?? process.platform
);
const appVersion = JSON.parse(
  readFileSync(path.resolve(__dirname, "package.json"), "utf8")
).version;

function normalizeTargetPlatform(platform?: string) {
  const normalized = platform?.toLowerCase();

  if (normalized === "darwin" || normalized === "macos") {
    return "macos";
  }
  if (normalized === "win32" || normalized === "windows") {
    return "windows";
  }
  if (normalized === "linux") {
    return "linux";
  }

  return normalized ?? "unknown";
}

const platformStylesFile =
  targetPlatform === "macos"
    ? "src/styles/platform/macos.css"
    : targetPlatform === "windows"
      ? "src/styles/platform/windows.css"
      : "src/styles/platform/linux.css";

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  define: {
    __TAURI_TARGET_PLATFORM__: JSON.stringify(targetPlatform),
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@platform-styles": path.resolve(__dirname, platformStylesFile),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
