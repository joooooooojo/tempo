import { defineConfig } from "vite";
import { tempoDevBridge } from "./tempo.vite";

export default defineConfig({
  plugins: [tempoDevBridge()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
