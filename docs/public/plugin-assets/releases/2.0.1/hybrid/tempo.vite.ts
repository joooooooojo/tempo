import fs from "node:fs";
import path from "node:path";
import type { Plugin } from "vite";

export function tempoDevBridge(): Plugin {
  return {
    name: "tempo-dev-bridge",
    apply: "serve",
    transformIndexHtml() {
      const root = process.cwd();
      return [
        {
          tag: "script",
          children: fs.readFileSync(
            path.join(root, ".tempo", "structured-clone.js"),
            "utf8",
          ),
          injectTo: "head-prepend",
        },
        {
          tag: "script",
          children: fs.readFileSync(
            path.join(root, ".tempo", "bridge-client.js"),
            "utf8",
          ),
          injectTo: "head-prepend",
        },
      ];
    },
  };
}

export function copyManifest(outDir = "dist"): Plugin {
  return {
    name: "tempo-copy-manifest",
    closeBundle() {
      fs.mkdirSync(outDir, { recursive: true });
      fs.copyFileSync("manifest.json", path.join(outDir, "manifest.json"));
    },
  };
}
