import path from "node:path";
import { defineConfig } from "vite";
import { tempoPlugin } from "tempo-plugin-sdk/vite";

export default defineConfig({
  plugins: [tempoPlugin()],
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
