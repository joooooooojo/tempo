import type { ReactNode } from "react";
import { LinkActionPopover, isSafeLinkHref } from "@/components/LinkActionPopover";

const URL_PATTERN = /https?:\/\/[^\s<>"'`]+|mailto:[^\s<>"'`]+/gi;

function trimTrailingPunctuation(url: string) {
  return url.replace(/[),.;:!?，。；！？】）》」』]+$/g, "");
}

export function TextWithLinks({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const nodes: ReactNode[] = [];
  let lastIndex = 0;
  const pattern = new RegExp(URL_PATTERN.source, URL_PATTERN.flags);
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    const raw = match[0];
    const href = trimTrailingPunctuation(raw);
    const start = match.index;
    if (start > lastIndex) {
      nodes.push(text.slice(lastIndex, start));
    }
    if (isSafeLinkHref(href)) {
      nodes.push(
        <LinkActionPopover key={`${start}-${href}`} href={href}>
          {href}
        </LinkActionPopover>
      );
      if (href.length < raw.length) {
        nodes.push(raw.slice(href.length));
      }
    } else {
      nodes.push(raw);
    }
    lastIndex = start + raw.length;
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }

  return <span className={className}>{nodes}</span>;
}
