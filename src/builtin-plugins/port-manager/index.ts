import { Cable } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { PortManagerPage } from "@/builtin-plugins/port-manager/pages/PortManagerPage";

export { PortManagerPage } from "@/builtin-plugins/port-manager/pages/PortManagerPage";

export const portManagerApp: TempoApp = reactApp({
  id: "port-manager",
  name: "端口管理器",
  keywords: ["port", "端口", "进程", "tcp", "udp"],
  icon: lucideIcon(Cable),
  component: wrapPage(PortManagerPage),
});
