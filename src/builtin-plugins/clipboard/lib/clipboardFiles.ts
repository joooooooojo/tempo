/** Parse clipboard history `kind: "file"` content (JSON path array). */
export function parseClipboardFilePaths(content: string): string[] {
  try {
    const parsed = JSON.parse(content) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map((value) => value.trim());
  } catch {
    return [];
  }
}

export function clipboardFileDisplayName(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] || path;
}

export function formatClipboardFilesPreview(paths: string[]): string {
  if (paths.length === 0) return "";
  if (paths.length === 1) return clipboardFileDisplayName(paths[0]);
  return `${clipboardFileDisplayName(paths[0])} 等 ${paths.length} 项`;
}

export function clipboardFileExtension(path: string): string {
  const name = clipboardFileDisplayName(path);
  const index = name.lastIndexOf(".");
  if (index <= 0 || index === name.length - 1) return "";
  return name.slice(index + 1).toUpperCase().slice(0, 4);
}

/** Footer / chip label: filename (multi → “a.pdf 等 N 项”). */
export function formatClipboardFilesFooter(paths: string[]): string {
  return formatClipboardFilesPreview(paths) || "文件";
}
