import { defineConfig } from "vite";
import { copyManifest, tempoDevBridge } from "./tempo.vite";

export default defineConfig({
  plugins: [tempoDevBridge(), copyManifest()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
