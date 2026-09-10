import { defineConfig } from "vite";
import { tempoPlugin } from "tempo-plugin-sdk/vite";

export default defineConfig({
  plugins: [tempoPlugin()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
