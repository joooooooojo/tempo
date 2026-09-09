import { fileIcon, type QuickAction, type QuickActionInput } from "@/apps/types";
import { api } from "@/lib/api";
import { extractClipboardUrl } from "@/lib/clipboardUrl";

export function clipboardInputHasUrl(input: QuickActionInput): boolean {
  if (input.kind !== "text") return false;
  return extractClipboardUrl(input.text) !== null;
}

export async function openClipboardLink(
  input: QuickActionInput,
  query: string,
  browserId: string | null,
  hideAndReset: () => Promise<void>,
) {
  const url =
    (input.kind === "text" ? extractClipboardUrl(input.text) : null) ??
    extractClipboardUrl(query);
  if (!url) return;
  await api.openUrlInBrowser(url, browserId);
  await hideAndReset();
}

/** Default browser — icon refreshed by `syncClipboardUrlBrowserActions`. */
export const openLinkAction: QuickAction = {
  id: "open-link",
  name: "打开链接",
  keywords: ["url", "link", "链接", "浏览器", "open"],
  icon: fileIcon(null),
  iconStyle: "app",
  source: "builtin",
  pluginId: "clipboard",
  accepts: ["text"],
  isVisible: clipboardInputHasUrl,
  /** Prefer over translate / todo when clipboard is a URL. */
  priority: 100,
  title: (query) => {
    const url = extractClipboardUrl(query);
    return url ? `打开链接：${url}` : "打开链接";
  },
  async run({ query, input, hideAndReset }) {
    await openClipboardLink(input, query, null, hideAndReset);
  },
  appIconName: "浏览器",
};
