import { TextQuote } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp } from "@/builtin-plugins/reactApp";
import { SnippetsPage } from "@/builtin-plugins/snippets/pages/SnippetsPage";

export { SnippetsPage } from "@/builtin-plugins/snippets/pages/SnippetsPage";

export const snippetsApp: TempoApp = reactApp({
  id: "snippets",
  name: "快捷短语",
  keywords: ["snippet", "短语", "快捷短语", "snippets"],
  icon: lucideIcon(TextQuote),
  component: SnippetsPage,
});
