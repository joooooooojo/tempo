import type { Plugin } from "vite";

const VIRTUAL_ID = "virtual:tempo-plugin-dev-bridge";
const RESOLVED_ID = `\0${VIRTUAL_ID}`;

/** Inject the standard Tempo development Bridge into Vite serve pages only. */
export function tempoPluginDev(): Plugin {
  return {
    name: "tempo-plugin-dev",
    apply: "serve",
    enforce: "pre",
    resolveId(id) {
      return id === VIRTUAL_ID ? RESOLVED_ID : null;
    },
    load(id) {
      return id === RESOLVED_ID ? 'import "@tempo/plugin-sdk/dev";' : null;
    },
    transformIndexHtml: {
      order: "pre",
      handler() {
        return [
          {
            tag: "script",
            attrs: {
              type: "module",
              src: `/@id/${RESOLVED_ID.replace("\0", "__x00__")}`,
            },
            injectTo: "head-prepend",
          },
        ];
      },
    },
  };
}
