import { ClipboardList } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { ClipboardPage } from "@/builtin-plugins/clipboard/pages/ClipboardPage";

export { ClipboardPage } from "@/builtin-plugins/clipboard/pages/ClipboardPage";
export { ShelfPickerPage } from "@/builtin-plugins/clipboard/pages/ShelfPickerPage";
export { ClipboardFileGlyph } from "@/builtin-plugins/clipboard/components/ClipboardFileGlyph";
export { formatClipboardFilesPreview } from "@/builtin-plugins/clipboard/lib/clipboardFiles";
export {
  resolveQuickActionInput,
  resolveQuickActionQuery,
  seedToMainPanelChip,
  shouldInlineClipboardText,
  type MainPanelClipboardChip,
} from "@/builtin-plugins/clipboard/lib/mainPanelClipboardSeed";

export const clipboardApp: TempoApp = reactApp({
  id: "clipboard",
  name: "剪贴板",
  keywords: ["clipboard", "剪贴板", "复制"],
  icon: lucideIcon(ClipboardList),
  component: wrapPage(ClipboardPage),
});
