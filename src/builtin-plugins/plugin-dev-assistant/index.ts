import { Braces } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { PluginDevAssistantPage } from "@/builtin-plugins/plugin-dev-assistant/pages/PluginDevAssistantPage";

export { PluginDevAssistantPage } from "@/builtin-plugins/plugin-dev-assistant/pages/PluginDevAssistantPage";

export const pluginDevAssistantApp: TempoApp = reactApp({
  id: "plugin-dev-assistant",
  name: "插件开发助手",
  keywords: ["plugin", "插件", "开发", "manifest", "headless"],
  icon: lucideIcon(Braces),
  component: wrapPage(PluginDevAssistantPage),
});
