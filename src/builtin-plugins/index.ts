import { registerApp } from "@/apps/registry";
import { BUILTIN_OWNER } from "@/apps/constants";
import type { TempoApp } from "@/apps/types";
import { todoApp } from "@/builtin-plugins/todo";
import { reportsApp } from "@/builtin-plugins/reports";
import { clipboardApp } from "@/builtin-plugins/clipboard";
import { snippetsApp } from "@/builtin-plugins/snippets";
import { hostsApp } from "@/builtin-plugins/hosts";
import { translateApp } from "@/builtin-plugins/translate";
import { portManagerApp } from "@/builtin-plugins/port-manager";
import { pluginDevAssistantApp } from "@/builtin-plugins/plugin-dev-assistant";
import { settingsApp } from "@/builtin-plugins/settings";

export { TODO_TITLE_LIMIT, BUILTIN_QUICK_ACTIONS } from "@/builtin-plugins/actions";

export const BUILTIN_APP_DEFS: TempoApp[] = [
  todoApp,
  reportsApp,
  clipboardApp,
  snippetsApp,
  hostsApp,
  translateApp,
  portManagerApp,
  pluginDevAssistantApp,
  settingsApp,
];

export const BUILTIN_APPS: TempoApp[] = BUILTIN_APP_DEFS;

let registered = false;

/** Register all built-in apps into the host app registry (idempotent). */
export function registerBuiltinPlugins(): void {
  if (registered) return;
  registered = true;
  for (const app of BUILTIN_APP_DEFS) {
    registerApp(BUILTIN_OWNER, app);
  }
}

registerBuiltinPlugins();
