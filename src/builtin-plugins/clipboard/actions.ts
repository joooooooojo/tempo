import {
  registerQuickAction,
  unregisterQuickAction,
} from "@/apps/actions/registry";
import { BUILTIN_OWNER } from "@/apps/constants";
import { fileIcon, type QuickAction } from "@/apps/types";
import {
  clipboardInputHasUrl,
  openClipboardLink,
} from "@/builtin-plugins/clipboard/openLink";
import { api } from "@/lib/api";
import { extractClipboardUrl } from "@/lib/clipboardUrl";

const BROWSER_ACTION_IDS = [
  "open-link-chrome",
  "open-link-edge",
  "open-link-firefox",
] as const;

function browserAction(
  id: (typeof BROWSER_ACTION_IDS)[number],
  browserId: string,
  actionName: string,
  browserName: string,
  iconDataUrl: string | null,
  keywords: string[],
): QuickAction {
  return {
    id,
    name: actionName,
    keywords: ["url", "link", "链接", "浏览器", ...keywords],
    icon: fileIcon(iconDataUrl),
    iconStyle: "app",
    source: "builtin",
    pluginId: "clipboard",
    accepts: ["text"],
    isVisible: clipboardInputHasUrl,
    /** Prefer over translate / todo when clipboard is a URL (just under default open-link). */
    priority: 90,
    title: (query) => {
      const url = extractClipboardUrl(query);
      return url ? `${actionName}：${url}` : actionName;
    },
    async run({ query, input, hideAndReset }) {
      await openClipboardLink(input, query, browserId, hideAndReset);
    },
    // Used by AppIcon fallback when the image fails to load.
    ...(browserName ? { appIconName: browserName } : {}),
  };
}

/**
 * Register Chrome / Edge / Firefox open-link actions when those browsers exist,
 * and refresh the default-browser icon for「打开链接」.
 * Safe to call repeatedly (e.g. on main panel open).
 */
export async function syncClipboardUrlBrowserActions(): Promise<void> {
  for (const id of BROWSER_ACTION_IDS) {
    unregisterQuickAction(id, BUILTIN_OWNER);
  }

  let browsers: Array<{
    id: string;
    actionName: string;
    name: string;
    iconDataUrl: string | null;
  }> = [];
  try {
    browsers = await api.listInstalledUrlBrowsers();
  } catch {
    return;
  }

  for (const browser of browsers) {
    const actionId = `open-link-${browser.id}` as (typeof BROWSER_ACTION_IDS)[number];
    if (!BROWSER_ACTION_IDS.includes(actionId)) continue;
    registerQuickAction(
      browserAction(actionId, browser.id, browser.actionName, browser.name, browser.iconDataUrl, [
        browser.id,
        browser.actionName,
      ]),
      BUILTIN_OWNER,
    );
  }

  try {
    const defaultBrowser = await api.getDefaultUrlBrowser();
    registerQuickAction(
      {
        id: "open-link",
        name: "打开链接",
        keywords: ["url", "link", "链接", "浏览器", "open"],
        icon: fileIcon(defaultBrowser?.iconDataUrl ?? null),
        iconStyle: "app",
        source: "builtin",
        pluginId: "clipboard",
        accepts: ["text"],
        isVisible: clipboardInputHasUrl,
        priority: 100,
        title: (query) => {
          const url = extractClipboardUrl(query);
          return url ? `打开链接：${url}` : "打开链接";
        },
        async run({ query, input, hideAndReset }) {
          await openClipboardLink(input, query, null, hideAndReset);
        },
        ...(defaultBrowser?.name ? { appIconName: defaultBrowser.name } : {}),
      },
      BUILTIN_OWNER,
    );
  } catch {
    /* keep statically registered open-link action */
  }
}
