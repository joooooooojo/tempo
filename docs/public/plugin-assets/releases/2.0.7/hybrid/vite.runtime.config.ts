import path from "node:path";
import { defineConfig } from "vite";
import { copyManifest } from "./tempo.vite";

export default defineConfig({
  plugins: [copyManifest()],
  build: {
    ssr: path.resolve("src/runtime/main.ts"),
    outDir: "dist",
    emptyOutDir: false,
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
