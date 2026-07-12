import { cn, formatRelativeTime, previewLines } from "@/lib/utils";

type ShelfCardProps = {
  selected?: boolean;
  headerLabel: string;
  headerTone: "text" | "image" | "snippet";
  timeLabel: string;
  sourceApp?: string | null;
  content: string;
  imageSrc?: string | null;
  footer: string;
  onClick?: () => void;
  onDoubleClick?: () => void;
};

const headerToneClass = {
  text: "shelf-card__header--text",
  image: "shelf-card__header--image",
  snippet: "shelf-card__header--snippet",
} as const;

export function ShelfCard({
  selected = false,
  headerLabel,
  headerTone,
  timeLabel,
  sourceApp,
  content,
  imageSrc,
  footer,
  onClick,
  onDoubleClick,
}: ShelfCardProps) {
  return (
    <button
      type="button"
      className={cn("shelf-card", selected && "shelf-card--selected")}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
    >
      <div className={cn("shelf-card__header", headerToneClass[headerTone])}>
        <span className="shelf-card__type">{headerLabel}</span>
        <span className="shelf-card__time">{timeLabel}</span>
        {sourceApp && (
          <span className="shelf-card__app" title={sourceApp}>
            {sourceApp}
          </span>
        )}
      </div>
      <div className="shelf-card__body">
        {imageSrc ? (
          <img src={imageSrc} alt="" className="shelf-card__image" />
        ) : (
          <p className="shelf-card__preview">{previewLines(content)}</p>
        )}
      </div>
      <div className="shelf-card__footer">{footer}</div>
    </button>
  );
}

export function shelfTimeLabel(iso: string) {
  return formatRelativeTime(iso);
}

export function shelfCharCount(text: string) {
  const count = [...text].length;
  return `${count} 个字符`;
}

export function shelfImageSize(width?: number | null, height?: number | null) {
  if (!width || !height) return "图片";
  return `${width} × ${height}`;
}

export function clipboardKindLabel(kind: string) {
  return kind === "image" ? "图片" : "文本";
}

export function clipboardHeaderTone(kind: string): "text" | "image" {
  return kind === "image" ? "image" : "text";
}

export function clipboardSourceLabel(entry: {
  source_app?: string | null;
  source_process?: string | null;
}) {
  if (entry.source_app) return entry.source_app;
  if (entry.source_process) return entry.source_process;
  return "未知来源";
}
