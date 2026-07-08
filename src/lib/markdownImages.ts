import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "@/lib/api";

export const MARKDOWN_IMAGE_MAX_BYTES = 5 * 1024 * 1024;

const SUPPORTED_IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"];

type ClipboardLike = {
  clipboardData: DataTransfer;
};

export function clipboardHasImages(event: ClipboardLike) {
  return Array.from(event.clipboardData.items).some(
    (item) => item.kind === "file" && item.type.startsWith("image/")
  );
}

export async function markdownImagesFromClipboard(event: ClipboardLike, alt = "图片") {
  const files = Array.from(event.clipboardData.items)
    .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file));

  const markdown: string[] = [];
  const errors: string[] = [];

  for (const file of files) {
    try {
      markdown.push(await markdownImageFromBlob(file, alt));
    } catch (error) {
      errors.push(error instanceof Error ? error.message : "图片读取失败");
    }
  }

  return { markdown: markdown.join("\n\n"), errors };
}

export async function markdownImageFromBlob(blob: Blob, alt = "图片") {
  if (blob.size > MARKDOWN_IMAGE_MAX_BYTES) {
    throw new Error("单张图片不能超过 5MB");
  }

  if (!SUPPORTED_IMAGE_TYPES.includes(blob.type)) {
    throw new Error("仅支持 PNG、JPEG、WebP 或 GIF 图片");
  }

  const filePath = await api.saveMarkdownImage(await readFileAsDataUrl(blob), blob.type);
  return `![${escapeMarkdownAlt(alt)}](${convertFileSrc(filePath)})`;
}

export function insertTextAtSelection(value: string, insertion: string, start: number, end: number) {
  const needsLeadingBreak = start > 0 && value[start - 1] !== "\n";
  const needsTrailingBreak = end < value.length && value[end] !== "\n";
  const text = `${needsLeadingBreak ? "\n\n" : ""}${insertion}${needsTrailingBreak ? "\n\n" : ""}`;
  return `${value.slice(0, start)}${text}${value.slice(end)}`;
}

function readFileAsDataUrl(file: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("图片读取失败"));
    reader.readAsDataURL(file);
  });
}

function escapeMarkdownAlt(value: string) {
  return value.replace(/[\[\]\\]/g, "");
}
