import { BarChart3 } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { ReportsPage } from "@/builtin-plugins/reports/pages/ReportsPage";

export { ReportsPage } from "@/builtin-plugins/reports/pages/ReportsPage";

export const reportsApp: TempoApp = reactApp({
  id: "reports",
  name: "屏幕使用时间",
  keywords: ["screen", "报告", "屏幕", "使用时间", "reports"],
  icon: lucideIcon(BarChart3),
  component: wrapPage(ReportsPage),
});
