import { isSafeLinkHref } from "@/components/LinkActionPopover";

function trimTrailingPunctuation(url: string) {
  return url.replace(/[),.;:!?，。；！？】）》」』]+$/g, "");
}

function isHttpOrHttps(href: string): boolean {
  try {
    const url = new URL(href);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Extract a browser-openable http(s) URL from clipboard / query text.
 * Accepts a whole-string URL, `www.…`, or the first http(s) match in the text.
 */
export function extractClipboardUrl(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;

  const candidates: string[] = [trimmed];
  if (/^www\./i.test(trimmed)) {
    candidates.push(`https://${trimmed}`);
  }

  for (const candidate of candidates) {
    const normalized = trimTrailingPunctuation(candidate);
    if (isHttpOrHttps(normalized)) return normalized;
    // mailto is a "safe link" but not for browser open actions
    if (isSafeLinkHref(normalized) && /^mailto:/i.test(normalized)) {
      continue;
    }
  }

  const match = /https?:\/\/[^\s<>"'`]+/i.exec(trimmed);
  if (!match) return null;
  const href = trimTrailingPunctuation(match[0]);
  return isHttpOrHttps(href) ? href : null;
}

export function clipboardTextHasUrl(text: string): boolean {
  return extractClipboardUrl(text) !== null;
}

export function quickActionInputUrl(
  input: { kind: string; text?: string },
  query: string,
): string | null {
  if (input.kind === "text" && typeof input.text === "string") {
    return extractClipboardUrl(input.text) ?? extractClipboardUrl(query);
  }
  return extractClipboardUrl(query);
}
