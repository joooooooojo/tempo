import fs from "node:fs";
import path from "node:path";
import type { Plugin } from "vite";

export function copyManifest(outDir = "dist"): Plugin {
  return {
    name: "tempo-copy-manifest",
    closeBundle() {
      fs.mkdirSync(outDir, { recursive: true });
      fs.copyFileSync("manifest.json", path.join(outDir, "manifest.json"));
    },
  };
}
