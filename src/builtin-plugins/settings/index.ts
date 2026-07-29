import { Settings } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { SettingsPage } from "@/builtin-plugins/settings/pages/SettingsPage";

export { SettingsPage } from "@/builtin-plugins/settings/pages/SettingsPage";
export {
  getBuiltinConfigPanel,
  hasBuiltinConfigPanel,
} from "@/builtin-plugins/settings/pages/config-registry";

export const settingsApp: TempoApp = reactApp({
  id: "settings",
  name: "设置",
  keywords: ["settings", "设置", "偏好", "配置"],
  icon: lucideIcon(Settings),
  component: wrapPage(SettingsPage),
});
