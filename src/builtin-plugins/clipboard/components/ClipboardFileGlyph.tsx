import { cn } from "@/lib/utils";
import { clipboardFileDisplayName, clipboardFileExtension } from "@/builtin-plugins/clipboard/lib/clipboardFiles";

type ClipboardFileGlyphProps = {
  paths: string[];
  size?: "card" | "chip" | "panel";
  className?: string;
};

export function ClipboardFileGlyph({
  paths,
  size = "card",
  className,
}: ClipboardFileGlyphProps) {
  const primary = paths[0] ? clipboardFileDisplayName(paths[0]) : "文件";
  const ext = paths[0] ? clipboardFileExtension(paths[0]) : "";
  const count = paths.length;

  return (
    <div
      className={cn(
        "clipboard-file-glyph",
        `clipboard-file-glyph--${size}`,
        className
      )}
      aria-hidden="true"
    >
      <div className="clipboard-file-glyph__sheet">
        <span className="clipboard-file-glyph__fold" />
        {ext ? <span className="clipboard-file-glyph__ext">{ext}</span> : null}
        {count > 1 ? (
          <span className="clipboard-file-glyph__count">+{count - 1}</span>
        ) : null}
      </div>
      {size === "panel" ? (
        <span className="clipboard-file-glyph__name" title={primary}>
          {primary}
        </span>
      ) : null}
    </div>
  );
}
