import path from "node:path";
import { defineConfig } from "vite";
import { copyManifest } from "./tempo.vite";

export default defineConfig({
  plugins: [copyManifest()],
  build: {
    ssr: path.resolve("src/main.ts"),
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: "main.mjs",
      },
    },
  },
  ssr: {
    noExternal: true,
  },
});
