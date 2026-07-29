import type { PluginDevLogEvent } from "@/types";

/** Mirror dev Runtime output to the host DevTools console. */
export function mirrorPluginDevLogToConsole(event: PluginDevLogEvent) {
  const label = `[plugin-dev:${event.source}]`;
  if (event.source === "stderr") {
    console.error(label, event.message);
    return;
  }
  console.log(label, event.message);
}
