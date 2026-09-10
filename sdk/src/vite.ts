import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export type TempoHtmlTag = {
  tag: string;
  children: string;
  injectTo: "head-prepend" | "head" | "body-prepend" | "body";
};

export type TempoViteConfig = {
  command: "serve" | "build";
  root: string;
  build: {
    outDir: string;
  };
};

export type TempoVitePlugin = {
  name: string;
  apply?: "serve" | "build";
  enforce?: "pre" | "post";
  transformIndexHtml?: () => TempoHtmlTag[];
  configResolved?: (config: TempoViteConfig) => void;
  closeBundle?: () => void;
};

const devAssetsRoot = fileURLToPath(new URL("./dev-assets/", import.meta.url));

function readDevAsset(fileName: string): string {
  return fs.readFileSync(path.join(devAssetsRoot, fileName), "utf8");
}

export function tempoPlugin(options: { outDir?: string } = {}): TempoVitePlugin {
  let command: "serve" | "build" = "build";
  let source = "";
  let target = "";

  return {
    name: "tempo-plugin",
    enforce: "pre",
    configResolved(config: TempoViteConfig) {
      command = config.command;
      source = path.join(config.root, "manifest.json");
      target = path.resolve(
        config.root,
        options.outDir ?? config.build.outDir,
        "manifest.json",
      );
    },
    transformIndexHtml() {
      if (command !== "serve") return [];
      return [
        {
          tag: "script",
          children: readDevAsset("structured-clone.js"),
          injectTo: "head-prepend",
        },
        {
          tag: "script",
          children: readDevAsset("bridge-client.js"),
          injectTo: "head-prepend",
        },
      ];
    },
    closeBundle() {
      if (command !== "build") return;
      if (!source || !target) {
        throw new Error("tempo-plugin was not initialized by Vite");
      }
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.copyFileSync(source, target);
    },
  };
}
