import { Languages } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp } from "@/builtin-plugins/reactApp";
import { TranslatePage } from "@/builtin-plugins/translate/pages/TranslatePage";

export { TranslatePage } from "@/builtin-plugins/translate/pages/TranslatePage";

export const translateApp: TempoApp = reactApp({
  id: "translate",
  name: "聚合翻译",
  keywords: ["translate", "翻译", "有道", "deepl"],
  icon: lucideIcon(Languages),
  component: TranslatePage,
});
