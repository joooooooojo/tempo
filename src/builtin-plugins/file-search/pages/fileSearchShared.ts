export const FILE_SEARCH_CATEGORIES = [
  { id: "all", label: "全部" },
  { id: "folder", label: "文件夹" },
  { id: "excel", label: "EXCEL" },
  { id: "word", label: "WORD" },
  { id: "ppt", label: "PPT" },
  { id: "pdf", label: "PDF" },
  { id: "image", label: "图片" },
  { id: "video", label: "视频" },
  { id: "audio", label: "音频" },
  { id: "archive", label: "压缩文件" },
] as const;

export type FileSearchCategoryId = (typeof FILE_SEARCH_CATEGORIES)[number]["id"];

export const FILE_SEARCH_SORTS = [
  { id: "mtime_desc", label: "修改时间 ↓" },
  { id: "mtime_asc", label: "修改时间 ↑" },
  { id: "name_asc", label: "名称 A-Z" },
  { id: "name_desc", label: "名称 Z-A" },
  { id: "size_desc", label: "大小 ↓" },
  { id: "size_asc", label: "大小 ↑" },
] as const;

export type FileSearchSortId = (typeof FILE_SEARCH_SORTS)[number]["id"];

export function formatFileSize(bytes?: number | null) {
  if (bytes == null || !Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function formatModifiedAt(value?: string | null) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value.replace("T", " ").slice(0, 19);
  }
  return date.toLocaleString();
}

export function truncateMiddle(path: string, max = 52) {
  if (path.length <= max) return path;
  const keep = Math.floor((max - 1) / 2);
  return `${path.slice(0, keep)}…${path.slice(-keep)}`;
}
