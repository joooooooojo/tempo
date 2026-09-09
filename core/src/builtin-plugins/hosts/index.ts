import { FileCode2 } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { HostsPage } from "@/builtin-plugins/hosts/pages/HostsPage";

export { HostsPage } from "@/builtin-plugins/hosts/pages/HostsPage";

export const hostsApp: TempoApp = reactApp({
  id: "hosts",
  name: "Hosts",
  keywords: ["hosts", "host", "域名"],
  icon: lucideIcon(FileCode2),
  component: wrapPage(HostsPage),
});
