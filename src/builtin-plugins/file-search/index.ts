import { Search } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { FileSearchPage } from "@/builtin-plugins/file-search/pages/FileSearchPage";

export { FileSearchPage } from "@/builtin-plugins/file-search/pages/FileSearchPage";

export const fileSearchApp: TempoApp = reactApp({
  id: "file-search",
  name: "文件搜索",
  keywords: ["file", "search", "文件", "搜索", "everything", "fd", "全盘"],
  icon: lucideIcon(Search),
  component: wrapPage(FileSearchPage),
});
