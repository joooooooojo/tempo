import { useMemo } from "react";
import hljs from "highlight.js";
import { cn } from "@/lib/utils";
import "highlight.js/styles/github.min.css";

export type SnippetLanguageOption = {
  value: string;
  label: string;
};

/** Language picker options derived from highlight.js registered languages. */
export const SNIPPET_LANGUAGE_OPTIONS: SnippetLanguageOption[] = [
  { value: "plain", label: "纯文本" },
  ...hljs
    .listLanguages()
    .map((id) => ({
      value: id,
      label: hljs.getLanguage(id)?.name || id,
    }))
    .sort((a, b) => a.label.localeCompare(b.label, "en", { sensitivity: "base" })),
];

export function highlightCode(code: string, language?: string | null) {
  const lang = language?.trim().toLowerCase();
  if (!lang || lang === "plain") {
    return escapeHtml(code);
  }
  try {
    if (hljs.getLanguage(lang)) {
      return hljs.highlight(code, { language: lang }).value;
    }
  } catch {
    // fall through
  }
  return escapeHtml(code);
}

export function CodeHighlight({
  code,
  language,
  className,
  maxLines,
  overflow = true,
}: {
  code: string;
  language?: string | null;
  className?: string;
  maxLines?: number;
  /** When false, skip internal scroll so a parent ScrollArea can own scrolling. */
  overflow?: boolean;
}) {
  const display = useMemo(() => {
    if (maxLines && maxLines > 0) {
      return code.split("\n").slice(0, maxLines).join("\n");
    }
    return code;
  }, [code, maxLines]);

  const html = useMemo(() => highlightCode(display, language), [display, language]);

  return (
    <pre
      className={cn(
        "code-highlight rounded-md border border-border/50 bg-muted/40 p-3 text-[12px] leading-5 text-foreground",
        overflow && "overflow-x-auto",
        className
      )}
    >
      <code
        className={cn(
          "hljs code-editor__hljs block overflow-visible whitespace-pre font-mono text-[12px] leading-5",
          language && `language-${language}`
        )}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </pre>
  );
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
