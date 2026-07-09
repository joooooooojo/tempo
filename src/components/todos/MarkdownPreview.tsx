import { Fragment, type ReactNode } from "react";
import { cn } from "@/lib/utils";

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "code"; code: string; language?: string }
  | { type: "quote"; text: string }
  | { type: "ul"; items: MarkdownListItem[] }
  | { type: "ol"; items: MarkdownListItem[] }
  | { type: "table"; headers: string[]; rows: string[][]; aligns: Array<"left" | "center" | "right" | undefined> }
  | { type: "hr" };

type MarkdownListItem = {
  text: string;
  checked?: boolean;
};

type MarkdownPreviewProps = {
  value: string;
  className?: string;
  emptyText?: string;
  onImagePreview?: (src: string, alt: string) => void;
};

const inlinePattern =
  /(!?\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)|`([^`]+)`|~~([^~]+)~~|\*\*([^*]+)\*\*|__([^_]+)__|\*([^*]+)\*|_([^_]+)_)/g;

export function MarkdownPreview({ value, className, emptyText = "暂无待办内容", onImagePreview }: MarkdownPreviewProps) {
  const blocks = parseMarkdown(value);

  if (blocks.length === 0) {
    return (
      <div className={cn("rounded-lg border border-dashed border-border/70 bg-foreground/[0.025] px-3 py-4 text-sm text-muted-foreground", className)}>
        {emptyText}
      </div>
    );
  }

  return (
    <div className={cn("github-markdown", className)}>
      {blocks.map((block, index) => renderBlock(block, index, onImagePreview))}
    </div>
  );
}

function parseMarkdown(value: string) {
  const lines = value.replace(/\r\n?/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let paragraph: string[] = [];
  let index = 0;

  const flushParagraph = () => {
    const text = paragraph.join("\n").trim();
    if (text) blocks.push({ type: "paragraph", text });
    paragraph = [];
  };

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();

    if (!trimmed) {
      flushParagraph();
      index += 1;
      continue;
    }

    const fence = trimmed.match(/^```(\w+)?\s*$/);
    if (fence) {
      flushParagraph();
      const code: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        code.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      blocks.push({ type: "code", code: code.join("\n"), language: fence[1] });
      continue;
    }

    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      flushParagraph();
      blocks.push({ type: "hr" });
      index += 1;
      continue;
    }

    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      index += 1;
      continue;
    }

    if (isTableHeader(lines, index)) {
      flushParagraph();
      const table = parseTable(lines, index);
      blocks.push(table.block);
      index = table.nextIndex;
      continue;
    }

    if (trimmed.startsWith(">")) {
      flushParagraph();
      const quoted: string[] = [];
      while (index < lines.length && lines[index].trim().startsWith(">")) {
        quoted.push(lines[index].trim().replace(/^>\s?/, ""));
        index += 1;
      }
      blocks.push({ type: "quote", text: quoted.join("\n").trim() });
      continue;
    }

    if (/^[-*+]\s+/.test(trimmed)) {
      flushParagraph();
      const items: MarkdownListItem[] = [];
      while (index < lines.length && /^[-*+]\s+/.test(lines[index].trim())) {
        items.push(parseListItem(lines[index].trim().replace(/^[-*+]\s+/, "")));
        index += 1;
      }
      blocks.push({ type: "ul", items });
      continue;
    }

    if (/^\d+\.\s+/.test(trimmed)) {
      flushParagraph();
      const items: MarkdownListItem[] = [];
      while (index < lines.length && /^\d+\.\s+/.test(lines[index].trim())) {
        items.push(parseListItem(lines[index].trim().replace(/^\d+\.\s+/, "")));
        index += 1;
      }
      blocks.push({ type: "ol", items });
      continue;
    }

    paragraph.push(line);
    index += 1;
  }

  flushParagraph();
  return blocks;
}

function isTableHeader(lines: string[], index: number) {
  const header = lines[index]?.trim() ?? "";
  const separator = lines[index + 1]?.trim() ?? "";
  if (!header.includes("|") || !separator.includes("|")) return false;

  const headerCells = splitTableRow(header);
  const separatorCells = splitTableRow(separator);
  return (
    headerCells.length > 0 &&
    separatorCells.length >= headerCells.length &&
    separatorCells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()))
  );
}

function parseTable(lines: string[], index: number) {
  const headers = splitTableRow(lines[index]);
  const aligns = splitTableRow(lines[index + 1]).map((cell) => {
    const trimmed = cell.trim();
    if (trimmed.startsWith(":") && trimmed.endsWith(":")) return "center";
    if (trimmed.endsWith(":")) return "right";
    return trimmed.startsWith(":") ? "left" : undefined;
  });
  const rows: string[][] = [];
  let nextIndex = index + 2;

  while (nextIndex < lines.length && lines[nextIndex].trim().includes("|")) {
    const row = splitTableRow(lines[nextIndex]);
    if (row.length === 0) break;
    rows.push(headers.map((_, columnIndex) => row[columnIndex] ?? ""));
    nextIndex += 1;
  }

  return {
    block: { type: "table" as const, headers, rows, aligns },
    nextIndex,
  };
}

function splitTableRow(row: string) {
  return row
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());
}

function parseListItem(text: string): MarkdownListItem {
  const task = text.match(/^\[( |x|X)\]\s+(.+)$/);
  if (!task) return { text };

  return {
    checked: task[1].toLowerCase() === "x",
    text: task[2],
  };
}

function renderBlock(block: MarkdownBlock, index: number, onImagePreview?: (src: string, alt: string) => void) {
  if (block.type === "heading") {
    const content = renderInline(block.text, onImagePreview);

    if (block.level === 1) return <h1 key={index}>{content}</h1>;
    if (block.level === 2) return <h2 key={index}>{content}</h2>;
    if (block.level === 3) return <h3 key={index}>{content}</h3>;
    if (block.level === 4) return <h4 key={index}>{content}</h4>;
    if (block.level === 5) return <h5 key={index}>{content}</h5>;
    return <h6 key={index}>{content}</h6>;
  }

  if (block.type === "paragraph") {
    return (
      <p key={index}>
        {renderInline(block.text, onImagePreview)}
      </p>
    );
  }

  if (block.type === "code") {
    return (
      <pre key={index}>
        {block.language && <div className="github-markdown-code-lang">{block.language}</div>}
        <code>{block.code}</code>
      </pre>
    );
  }

  if (block.type === "quote") {
    return (
      <blockquote key={index}>
        {renderInline(block.text, onImagePreview)}
      </blockquote>
    );
  }

  if (block.type === "ul") {
    return (
      <ul key={index} className={block.items.some((item) => item.checked !== undefined) ? "contains-task-list" : undefined}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex} className={item.checked !== undefined ? "task-list-item" : undefined}>
            {item.checked !== undefined && <input type="checkbox" checked={item.checked} readOnly />}
            {renderInline(item.text, onImagePreview)}
          </li>
        ))}
      </ul>
    );
  }

  if (block.type === "ol") {
    return (
      <ol key={index}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex}>{renderInline(item.text, onImagePreview)}</li>
        ))}
      </ol>
    );
  }

  if (block.type === "table") {
    return (
      <div key={index} className="github-markdown-table-wrap">
        <table>
          <thead>
            <tr>
              {block.headers.map((header, headerIndex) => (
                <th key={headerIndex} style={{ textAlign: block.aligns[headerIndex] }}>
                  {renderInline(header, onImagePreview)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, rowIndex) => (
              <tr key={rowIndex}>
                {block.headers.map((_, columnIndex) => (
                  <td key={columnIndex} style={{ textAlign: block.aligns[columnIndex] }}>
                    {renderInline(row[columnIndex] ?? "", onImagePreview)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  return <hr key={index} />;
}

function renderInline(text: string, onImagePreview?: (src: string, alt: string) => void) {
  const nodes: ReactNode[] = [];
  let lastIndex = 0;

  inlinePattern.lastIndex = 0;
  for (const match of text.matchAll(inlinePattern)) {
    if (match.index === undefined) continue;
    if (match.index > lastIndex) {
      nodes.push(renderTextWithBreaks(text.slice(lastIndex, match.index), nodes.length));
    }

    const full = match[0];
    const isImage = full.startsWith("![");
    if (match[1] && isImage) {
      const alt = match[2] || "Markdown image";
      const src = match[3];
      nodes.push(renderImage(src, alt, nodes.length, onImagePreview));
    } else if (match[1]) {
      const label = match[2] || match[3];
      const href = match[3];
      nodes.push(renderLink(href, label, nodes.length));
    } else if (match[4]) {
      nodes.push(
        <code key={nodes.length}>
          {match[4]}
        </code>
      );
    } else if (match[5]) {
      nodes.push(<del key={nodes.length}>{match[5]}</del>);
    } else {
      const strong = match[6] ?? match[7];
      const emphasis = match[8] ?? match[9];
      nodes.push(
        strong ? (
          <strong key={nodes.length} className="font-semibold text-foreground">
            {strong}
          </strong>
        ) : (
          <em key={nodes.length}>{emphasis}</em>
        )
      );
    }

    lastIndex = match.index + full.length;
  }

  if (lastIndex < text.length) {
    nodes.push(renderTextWithBreaks(text.slice(lastIndex), nodes.length));
  }

  return nodes;
}

function renderTextWithBreaks(text: string, keyPrefix: number) {
  const parts = text.split("\n");
  return parts.map((part, index) => (
    <Fragment key={`${keyPrefix}-${index}`}>
      {index > 0 && <br />}
      {part}
    </Fragment>
  ));
}

function renderImage(src: string, alt: string, key: number, onImagePreview?: (src: string, alt: string) => void) {
  if (!isSafeImageSrc(src)) {
    return (
      <span key={key} className="text-muted-foreground">
        {alt}
      </span>
    );
  }

  const image = (
    <img
      src={src}
      alt={alt}
      draggable={false}
    />
  );

  if (!onImagePreview) return <Fragment key={key}>{image}</Fragment>;

  return (
    <button
      key={key}
      type="button"
      className="github-markdown-image-button"
      onClick={() => onImagePreview(src, alt)}
      aria-label={`预览图片：${alt}`}
    >
      {image}
    </button>
  );
}

function renderLink(href: string, label: string, key: number) {
  if (!isSafeLinkHref(href)) return <span key={key}>{label}</span>;

  return (
    <a key={key} href={href} target="_blank" rel="noreferrer">
      {label}
    </a>
  );
}

function isSafeImageSrc(src: string) {
  if (/^data:image\/(png|jpeg|jpg|webp|gif);base64,/i.test(src)) return true;
  if (src.startsWith("blob:")) return true;
  if (isTauriAssetUrl(src)) return true;
  return isHttpUrl(src);
}

function isSafeLinkHref(href: string) {
  if (/^mailto:/i.test(href)) return true;
  return isHttpUrl(href);
}

function isHttpUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function isTauriAssetUrl(value: string) {
  try {
    const url = new URL(value);
    return (
      url.protocol === "asset:" ||
      url.hostname === "asset.localhost" ||
      url.protocol === "tempo-image:" ||
      url.hostname === "tempo-image.localhost"
    );
  } catch {
    return false;
  }
}
