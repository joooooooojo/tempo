import { useEffect, useRef, useState } from "react";
import { ExternalLink, FileIcon, FolderOpen, Loader2 } from "lucide-react";
import * as XLSX from "xlsx";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { api } from "@/lib/api";
import {
  formatFileSize,
  formatModifiedAt,
} from "@/builtin-plugins/file-search/pages/fileSearchShared";
import { renderDocxPreview } from "@/builtin-plugins/file-search/components/docxPreview";
import type {
  FileSearchArchiveListing,
  FileSearchItem,
  FileSearchPreviewKind,
  FileSearchPreviewMeta,
} from "@/types";

const OFFICE_MAX_BYTES = 15 * 1024 * 1024;
const TEXT_MAX_BYTES = 256 * 1024;
const TEXT_MAX_CHARS = 80_000;
const EXCEL_MAX_ROWS = 80;
const EXCEL_MAX_COLS = 20;

const ARCHIVE_EXTENSIONS = new Set([
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz",
  "jar", "apk", "whl", "war", "ear", "tbz", "tbz2", "txz",
]);

const TEXT_EXTENSIONS = new Set([
  "txt", "text", "log", "md", "markdown", "rst", "adoc", "asciidoc",
  "json", "jsonc", "json5", "yaml", "yml", "toml", "xml", "xsl", "xsd",
  "html", "htm", "xhtml", "css", "scss", "sass", "less", "styl",
  "ini", "cfg", "conf", "config", "env", "properties", "editorconfig",
  "gitignore", "gitattributes", "dockerignore", "npmrc", "yarnrc", "lock",
  "sh", "bash", "zsh", "fish", "ps1", "psm1", "psd1", "bat", "cmd",
  "rs", "go", "py", "pyi", "pyw", "rb", "php", "java", "kt", "kts",
  "swift", "scala", "cs", "fs", "fsx", "c", "h", "cc", "cpp", "cxx",
  "hpp", "hxx", "m", "mm", "js", "jsx", "mjs", "cjs", "ts", "tsx",
  "mts", "cts", "vue", "svelte", "astro", "lua", "r", "pl", "pm", "tcl",
  "groovy", "gradle", "cmake", "sql", "graphql", "gql", "proto", "dart",
  "ex", "exs", "erl", "hrl", "clj", "cljs", "edn", "hs", "elm", "zig",
  "nim", "v", "vb", "vbs", "diff", "patch", "csv", "tsv", "nfo", "srt", "vtt",
  "ass", "ssa", "tex", "bib", "dockerfile", "makefile", "mk",
]);

type ExcelTable = {
  sheetName: string;
  rows: string[][];
  truncated: boolean;
};

type OfficeStatus = {
  state: "idle" | "loading" | "ready" | "error";
  message?: string;
  /** OOXML bytes for styled Word render via docx-preview. */
  docxBuffer?: ArrayBuffer;
  table?: ExcelTable;
  bytes?: number;
};

type TextStatus = {
  state: "idle" | "loading" | "ready" | "error";
  content?: string;
  truncated?: boolean;
  message?: string;
};

type ArchiveStatus = {
  state: "idle" | "loading" | "ready" | "error";
  listing?: FileSearchArchiveListing;
  message?: string;
};

export function extensionFromPath(path: string, fallback?: string | null): string {
  if (fallback?.trim()) return fallback.trim().toLowerCase();
  const base = path.replace(/\\/g, "/").split("/").pop() ?? "";
  const index = base.lastIndexOf(".");
  if (index <= 0 || index === base.length - 1) return "";
  return base.slice(index + 1).toLowerCase();
}

export function resolvePreviewKind(
  meta: FileSearchPreviewMeta | null,
  item: FileSearchItem | null,
): FileSearchPreviewKind {
  if (!item || item.isDir) return "none";
  const ext = extensionFromPath(item.path, item.extension ?? meta?.extension);
  // CSV is plain text; never treat as Excel even if an older backend says "excel".
  if (ext === "csv") return "text";
  if (meta?.previewKind && meta.previewKind !== "none") return meta.previewKind;
  if (["jpg", "jpeg", "png", "gif", "bmp", "webp", "ico", "svg"].includes(ext)) return "image";
  if (["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v"].includes(ext)) return "video";
  if (["mp3", "wav", "flac", "aac", "m4a", "ogg", "wma", "aiff"].includes(ext)) return "audio";
  if (["xlsx", "xls", "xlsm", "xlsb"].includes(ext)) return "excel";
  if (["doc", "docx", "docm", "rtf", "odt"].includes(ext)) return "word";
  if (["ppt", "pptx", "pptm", "odp"].includes(ext)) return "ppt";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "archive";
  if (TEXT_EXTENSIONS.has(ext)) return "text";
  return "none";
}

function typeLabel(
  item: FileSearchItem,
  meta: FileSearchPreviewMeta | null,
  kind: FileSearchPreviewKind,
): string {
  if (item.isDir) return "文件夹";
  const ext = extensionFromPath(item.path, item.extension ?? meta?.extension);
  if (ext) return ext.toUpperCase();
  if (meta?.mimeHint) return meta.mimeHint;
  switch (kind) {
    case "image":
      return "图片";
    case "video":
      return "视频";
    case "audio":
      return "音频";
    case "text":
      return "文本";
    case "excel":
      return "Excel";
    case "word":
      return "Word";
    case "ppt":
      return "PPT";
    case "archive":
      return "压缩文件";
    default:
      return "文件";
  }
}

async function fetchPreviewBuffer(path: string): Promise<ArrayBuffer> {
  const src = await api.fileSearchPreviewUrl(path);
  const res = await fetch(src);
  if (!res.ok) {
    throw new Error(`无法读取本地文件（HTTP ${res.status}）`);
  }
  return res.arrayBuffer();
}

function looksBinary(bytes: Uint8Array): boolean {
  const sample = bytes.subarray(0, Math.min(bytes.length, 4096));
  let control = 0;
  for (let i = 0; i < sample.length; i += 1) {
    const b = sample[i]!;
    if (b === 0) return true;
    if (b < 7 || (b > 13 && b < 32)) control += 1;
  }
  return sample.length > 0 && control / sample.length > 0.3;
}

/** Friendlier copy when sniff fails; LevelDB WALs often use a `.log` suffix. */
function binaryPreviewMessage(path: string): string {
  const normalized = path.replace(/\\/g, "/").toLowerCase();
  if (normalized.includes("/leveldb/")) {
    return "该 .log 为 LevelDB 等二进制日志，非文本，无法预览";
  }
  return "内容疑似二进制，无法作为文本预览";
}

function decodeTextChunk(bytes: Uint8Array): string {
  try {
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    let out = "";
    for (let i = 0; i < bytes.length; i += 1) {
      out += String.fromCharCode(bytes[i]!);
    }
    return out;
  }
}

function parseExcelTable(buf: ArrayBuffer): ExcelTable {
  const workbook = XLSX.read(buf, { type: "array" });
  const sheetName = workbook.SheetNames[0] ?? "Sheet1";
  const sheet = workbook.Sheets[sheetName];
  if (!sheet) {
    return { sheetName, rows: [], truncated: false };
  }
  const raw = XLSX.utils.sheet_to_json<(string | number | boolean | null)[]>(sheet, {
    header: 1,
    defval: "",
    raw: false,
  });
  const truncated =
    raw.length > EXCEL_MAX_ROWS || raw.some((row) => (row?.length ?? 0) > EXCEL_MAX_COLS);
  const rows = raw.slice(0, EXCEL_MAX_ROWS).map((row) =>
    (row ?? []).slice(0, EXCEL_MAX_COLS).map((cell) => {
      if (cell == null) return "";
      return String(cell);
    }),
  );
  const colCount = Math.max(0, ...rows.map((row) => row.length));
  const normalized =
    colCount === 0
      ? rows
      : rows.map((row) =>
          row.length >= colCount
            ? row
            : [...row, ...Array.from({ length: colCount - row.length }, () => "")],
        );
  return { sheetName, rows: normalized, truncated };
}

function DocxStyledPreview({
  buffer,
  bytes,
  onOpen,
}: {
  buffer: ArrayBuffer;
  bytes?: number;
  onOpen: () => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [renderState, setRenderState] = useState<"loading" | "ready" | "error">("loading");
  const [renderError, setRenderError] = useState<string | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    let cancelled = false;
    setRenderState("loading");
    setRenderError(null);

    void (async () => {
      try {
        await renderDocxPreview(buffer, el);
        if (cancelled) return;
        setRenderState("ready");
      } catch (err) {
        if (cancelled) return;
        el.replaceChildren();
        setRenderState("error");
        setRenderError(err instanceof Error ? err.message : String(err));
      }
    })();

    return () => {
      cancelled = true;
      el.replaceChildren();
    };
  }, [buffer]);

  if (renderState === "error") {
    return (
      <div className="file-search-detail__office file-search-detail__office--cta">
        {bytes != null ? (
          <p className="file-search-detail__office-size">{formatFileSize(bytes)}</p>
        ) : null}
        <p className="file-search-detail__preview-hint">
          {renderError ? `预览失败：${renderError}` : "无法解析文档内容，请用系统应用打开。"}
        </p>
        <Button type="button" size="sm" onClick={onOpen}>
          <ExternalLink className="h-3.5 w-3.5" />
          打开
        </Button>
      </div>
    );
  }

  return (
    <div className="file-search-detail__office">
      <div className="file-search-detail__office-meta">
        <span>Word</span>
        {bytes != null ? <span>{formatFileSize(bytes)}</span> : null}
      </div>
      <ScrollArea
        className="file-search-preview-docx-scroll min-h-0 flex-1 w-full"
        scrollbars="both"
        viewportClassName="file-search-preview-docx-viewport"
      >
        <div className="file-search-preview-docx-measure">
          <div
            ref={containerRef}
            className="file-search-preview-docx"
            aria-busy={renderState === "loading"}
          />
        </div>
      </ScrollArea>
      {renderState === "loading" ? (
        <p className="file-search-detail__preview-hint" aria-busy="true">
          正在渲染文档…
        </p>
      ) : null}
    </div>
  );
}

function UnsupportedFallback({
  item,
  meta,
  message,
}: {
  item: FileSearchItem;
  meta: FileSearchPreviewMeta | null;
  message?: string;
}) {
  const kind = resolvePreviewKind(meta, item);
  return (
    <ScrollArea className="h-full min-h-0" viewportClassName="file-search-detail__fallback-viewport">
      <div className="file-search-detail__fallback">
        <div className="file-search-detail__fallback-icon">
          {item.isDir ? <FolderOpen className="h-8 w-8" /> : <FileIcon className="h-8 w-8" />}
        </div>
        <p className="file-search-detail__fallback-title">无法预览</p>
        <p className="file-search-detail__preview-hint">
          {message ?? (item.isDir ? "文件夹不支持内容预览" : "此文件类型暂不支持预览")}
        </p>
        <dl className="file-search-detail__fallback-meta">
          <div>
            <dt>名称</dt>
            <dd title={item.name}>{item.name}</dd>
          </div>
          <div>
            <dt>类型</dt>
            <dd>{typeLabel(item, meta, kind)}</dd>
          </div>
          <div>
            <dt>大小</dt>
            <dd>{formatFileSize(meta?.size ?? item.size)}</dd>
          </div>
          <div>
            <dt>修改时间</dt>
            <dd>{formatModifiedAt(meta?.modifiedAt ?? item.modifiedAt)}</dd>
          </div>
          <div>
            <dt>路径</dt>
            <dd title={item.path}>{item.path}</dd>
          </div>
        </dl>
      </div>
    </ScrollArea>
  );
}

export function FileSearchPreview({
  enabled,
  item,
  meta,
  metaLoading,
  metaError,
  onOpen,
}: {
  enabled: boolean;
  item: FileSearchItem | null;
  meta: FileSearchPreviewMeta | null;
  metaLoading: boolean;
  metaError: string | null;
  onOpen: () => void;
}) {
  const kind = resolvePreviewKind(meta, item);
  const [mediaSrc, setMediaSrc] = useState<string | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [office, setOffice] = useState<OfficeStatus>({ state: "idle" });
  const [text, setText] = useState<TextStatus>({ state: "idle" });
  const [archive, setArchive] = useState<ArchiveStatus>({ state: "idle" });

  useEffect(() => {
    setMediaError(null);
    setMediaSrc(null);
    if (!enabled || !item || item.isDir) return;
    if (kind !== "image" && kind !== "video" && kind !== "audio") return;

    let cancelled = false;
    void api
      .fileSearchPreviewUrl(item.path)
      .then((url) => {
        if (!cancelled) setMediaSrc(url);
      })
      .catch((err) => {
        if (!cancelled) {
          setMediaError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [enabled, item?.path, item?.isDir, kind]);

  useEffect(() => {
    if (!enabled || !item || item.isDir || kind !== "text") {
      setText({ state: "idle" });
      return;
    }

    let cancelled = false;
    setText({ state: "loading" });

    void (async () => {
      try {
        const knownSize = meta?.size ?? item.size ?? null;
        const src = await api.fileSearchPreviewUrl(item.path);
        const end = TEXT_MAX_BYTES - 1;
        const res = await fetch(src, {
          headers: { Range: `bytes=0-${end}` },
        });
        if (!res.ok && res.status !== 206) {
          throw new Error(`无法读取本地文件（HTTP ${res.status}）`);
        }
        const buf = await res.arrayBuffer();
        if (cancelled) return;
        const bytes = new Uint8Array(buf);
        if (looksBinary(bytes)) {
          setText({
            state: "error",
            message: binaryPreviewMessage(item.path),
          });
          return;
        }
        let content = decodeTextChunk(bytes);
        let truncated =
          (knownSize != null && knownSize > TEXT_MAX_BYTES) ||
          bytes.length >= TEXT_MAX_BYTES ||
          res.status === 206;
        if (content.length > TEXT_MAX_CHARS) {
          content = content.slice(0, TEXT_MAX_CHARS);
          truncated = true;
        }
        setText({
          state: "ready",
          content,
          truncated,
          message: truncated
            ? `仅显示前 ${formatFileSize(Math.min(TEXT_MAX_BYTES, bytes.length))}（已截断）`
            : undefined,
        });
      } catch (err) {
        if (cancelled) return;
        setText({
          state: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [enabled, item?.path, item?.isDir, item?.size, kind, meta?.size]);

  useEffect(() => {
    if (!enabled || !item || item.isDir) {
      setOffice({ state: "idle" });
      return;
    }
    if (kind !== "excel" && kind !== "word" && kind !== "ppt") {
      setOffice({ state: "idle" });
      return;
    }

    let cancelled = false;
    setOffice({ state: "loading" });

    void (async () => {
      try {
        const knownSize = meta?.size ?? item.size ?? null;
        if (knownSize != null && knownSize > OFFICE_MAX_BYTES) {
          if (cancelled) return;
          setOffice({
            state: "ready",
            bytes: knownSize,
            message: `文件较大（${formatFileSize(knownSize)}），跳过解析。请用系统应用打开。`,
          });
          return;
        }

        const buf = await fetchPreviewBuffer(item.path);
        if (cancelled) return;

        if (buf.byteLength > OFFICE_MAX_BYTES) {
          setOffice({
            state: "ready",
            bytes: buf.byteLength,
            message: `文件较大（${formatFileSize(buf.byteLength)}），跳过解析。请用系统应用打开。`,
          });
          return;
        }

        if (kind === "excel") {
          const table = parseExcelTable(buf);
          setOffice({
            state: "ready",
            bytes: buf.byteLength,
            table,
            message: table.truncated ? `仅显示前 ${EXCEL_MAX_ROWS} 行 / ${EXCEL_MAX_COLS} 列` : undefined,
          });
          return;
        }

        if (kind === "word") {
          const ext = extensionFromPath(item.path, item.extension ?? meta?.extension);
          // Old binary .doc / RTF / ODT are not parsed; only OOXML (.docx / .docm).
          if (ext !== "docx" && ext !== "docm") {
            if (cancelled) return;
            setOffice({
              state: "ready",
              bytes: buf.byteLength,
              message: "此 Word 格式暂不支持预览，请用系统应用打开。",
            });
            return;
          }
          setOffice({
            state: "ready",
            bytes: buf.byteLength,
            docxBuffer: buf,
          });
          return;
        }

        setOffice({
          state: "ready",
          bytes: buf.byteLength,
          message: "已加载文件流。幻灯片预览暂不支持，请用系统应用打开。",
        });
      } catch (err) {
        if (cancelled) return;
        setOffice({
          state: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    enabled,
    item?.path,
    item?.isDir,
    item?.size,
    item?.extension,
    kind,
    meta?.size,
    meta?.extension,
  ]);

  useEffect(() => {
    if (!enabled || !item || item.isDir || kind !== "archive") {
      setArchive({ state: "idle" });
      return;
    }

    let cancelled = false;
    setArchive({ state: "loading" });

    void (async () => {
      try {
        const listing = await api.fileSearchListArchive(item.path);
        if (cancelled) return;
        if (!listing.entries.length && listing.message) {
          setArchive({
            state: "error",
            message: listing.message,
            listing,
          });
          return;
        }
        setArchive({ state: "ready", listing });
      } catch (err) {
        if (cancelled) return;
        setArchive({
          state: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [enabled, item?.path, item?.isDir, kind]);

  if (!enabled) return null;

  if (!item) {
    return (
      <div className="file-search-detail__preview-status">
        <p className="file-search-detail__preview-hint">选择文件以预览</p>
      </div>
    );
  }

  if (metaLoading && kind === "none") {
    return (
      <div className="file-search-detail__preview-status" aria-busy="true">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (kind === "image") {
    if (mediaError || metaError) {
      return (
        <UnsupportedFallback
          item={item}
          meta={meta}
          message={mediaError ?? metaError ?? "无法加载预览"}
        />
      );
    }
    if (!mediaSrc) {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      );
    }
    return (
      <img
        key={mediaSrc}
        src={mediaSrc}
        alt=""
        className="file-search-detail__image"
        onError={() => setMediaError("无法加载本地图片")}
        onLoad={() => setMediaError(null)}
      />
    );
  }

  if (kind === "video") {
    if (mediaError) {
      return <UnsupportedFallback item={item} meta={meta} message={mediaError} />;
    }
    if (!mediaSrc) {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      );
    }
    return (
      <video
        key={mediaSrc}
        className="file-search-detail__video"
        src={mediaSrc}
        controls
        preload="metadata"
        onError={() => setMediaError("无法加载本地视频")}
      />
    );
  }

  if (kind === "audio") {
    if (mediaError) {
      return <UnsupportedFallback item={item} meta={meta} message={mediaError} />;
    }
    if (!mediaSrc) {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      );
    }
    return (
      <div className="file-search-detail__audio-wrap">
        <audio
          key={mediaSrc}
          className="file-search-detail__audio"
          src={mediaSrc}
          controls
          preload="metadata"
          onError={() => setMediaError("无法加载本地音频")}
        />
      </div>
    );
  }

  if (kind === "text") {
    if (text.state === "loading") {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          <p className="file-search-detail__preview-hint">正在加载文本…</p>
        </div>
      );
    }
    if (text.state === "error") {
      return (
        <UnsupportedFallback
          item={item}
          meta={meta}
          message={text.message ?? "无法预览"}
        />
      );
    }
    return (
      <div className="file-search-detail__text-pane">
        <ScrollArea
          className="file-search-detail__text-scroll min-h-0 flex-1"
          viewportClassName="file-search-detail__text-viewport"
        >
          <pre className="file-search-detail__text file-search-detail__text--pane">
            {text.content || "（空文件）"}
          </pre>
        </ScrollArea>
        {text.truncated || text.message ? (
          <p className="file-search-detail__preview-hint">
            {text.message ?? "内容已截断"}
          </p>
        ) : null}
      </div>
    );
  }

  if (kind === "excel" || kind === "word" || kind === "ppt") {
    if (office.state === "loading") {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          <p className="file-search-detail__preview-hint">正在加载预览…</p>
        </div>
      );
    }
    if (office.state === "error") {
      return (
        <UnsupportedFallback
          item={item}
          meta={meta}
          message={office.message ?? "预览失败"}
        />
      );
    }

    if (kind === "excel" && office.table) {
      const { rows, sheetName, truncated } = office.table;
      return (
        <div className="file-search-detail__office">
          <div className="file-search-detail__office-meta">
            <span>{sheetName}</span>
            {office.bytes != null ? <span>{formatFileSize(office.bytes)}</span> : null}
          </div>
          {rows.length === 0 ? (
            <p className="file-search-detail__preview-hint">工作表为空</p>
          ) : (
            <ScrollArea
              className="file-search-detail__table-scroll min-h-0 flex-1"
              scrollbars="both"
              viewportClassName="file-search-detail__table-viewport"
            >
              <table className="file-search-detail__table">
                <tbody>
                  {rows.map((row, rowIndex) => (
                    <tr key={rowIndex}>
                      {row.map((cell, colIndex) => (
                        <td key={colIndex}>{cell}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </ScrollArea>
          )}
          {(truncated || office.message) && (
            <p className="file-search-detail__preview-hint">
              {office.message ?? `仅显示前 ${EXCEL_MAX_ROWS} 行 / ${EXCEL_MAX_COLS} 列`}
            </p>
          )}
        </div>
      );
    }

    if (kind === "word" && office.docxBuffer) {
      return (
        <DocxStyledPreview
          buffer={office.docxBuffer}
          bytes={office.bytes}
          onOpen={onOpen}
        />
      );
    }

    return (
      <div className="file-search-detail__office file-search-detail__office--cta">
        {office.bytes != null ? (
          <p className="file-search-detail__office-size">{formatFileSize(office.bytes)}</p>
        ) : null}
        <p className="file-search-detail__preview-hint">
          {office.message ?? "请用系统应用打开"}
        </p>
        <Button type="button" size="sm" onClick={onOpen}>
          <ExternalLink className="h-3.5 w-3.5" />
          打开
        </Button>
      </div>
    );
  }

  if (kind === "archive") {
    if (archive.state === "loading") {
      return (
        <div className="file-search-detail__preview-status" aria-busy="true">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          <p className="file-search-detail__preview-hint">正在读取压缩包目录…</p>
        </div>
      );
    }
    if (archive.state === "error") {
      return (
        <UnsupportedFallback
          item={item}
          meta={meta}
          message={archive.message ?? "无法预览压缩包"}
        />
      );
    }

    const listing = archive.listing;
    if (!listing) {
      return (
        <UnsupportedFallback item={item} meta={meta} message="无法读取压缩包目录" />
      );
    }

    const formatLabel = listing.format.toUpperCase();
    return (
      <div className="file-search-detail__archive">
        <div className="file-search-detail__office-meta">
          <span>
            {formatLabel} · {listing.totalEntries} 项
          </span>
          {meta?.size != null || item.size != null ? (
            <span>{formatFileSize(meta?.size ?? item.size)}</span>
          ) : null}
        </div>
        {listing.entries.length === 0 ? (
          <p className="file-search-detail__preview-hint">压缩包为空</p>
        ) : (
          <ScrollArea
            className="file-search-detail__archive-scroll min-h-0 flex-1"
            viewportClassName="file-search-detail__archive-viewport"
          >
            <ul className="file-search-detail__archive-list">
              {listing.entries.map((entry, index) => (
                <li
                  key={`${entry.path}-${index}`}
                  className={
                    entry.isDir
                      ? "file-search-detail__archive-item file-search-detail__archive-item--dir"
                      : "file-search-detail__archive-item"
                  }
                  title={entry.path}
                >
                  <span className="file-search-detail__archive-name">
                    {entry.isDir ? (
                      <FolderOpen className="file-search-detail__archive-icon" />
                    ) : (
                      <FileIcon className="file-search-detail__archive-icon" />
                    )}
                    {entry.path || "（未命名）"}
                  </span>
                  <span className="file-search-detail__archive-size">
                    {entry.isDir
                      ? ""
                      : entry.compressedSize != null
                        ? formatFileSize(entry.compressedSize)
                        : entry.size != null
                          ? formatFileSize(entry.size)
                          : ""}
                  </span>
                </li>
              ))}
            </ul>
          </ScrollArea>
        )}
        {listing.message || listing.truncated ? (
          <p className="file-search-detail__preview-hint">
            {listing.message ??
              `仅显示前 ${listing.entries.length} 项（共 ${listing.totalEntries} 项）`}
          </p>
        ) : null}
      </div>
    );
  }

  return (
    <UnsupportedFallback
      item={item}
      meta={meta}
      message={metaError ?? undefined}
    />
  );
}
