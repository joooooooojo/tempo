import type { MainPanelClipboardSeed } from "@/types";
import type { QuickActionInput } from "@/apps/types";
import {
  formatClipboardFilesPreview,
} from "@/lib/clipboardFiles";

/** Short text goes straight into the search input; longer text uses a leading chip. */
export const MAIN_PANEL_CLIPBOARD_INLINE_MAX_LEN = 48;

const CHIP_HEAD = 14;
const CHIP_TAIL = 14;
const CHIP_MAX_LEN = 44;

export function truncateClipboardChipLabel(text: string): string {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (normalized.length <= CHIP_MAX_LEN) return normalized;
  return `${normalized.slice(0, CHIP_HEAD)}......${normalized.slice(-CHIP_TAIL)}`;
}

export function shouldInlineClipboardText(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;
  if (trimmed.includes("\n")) return false;
  return trimmed.length <= MAIN_PANEL_CLIPBOARD_INLINE_MAX_LEN;
}

export function resolveQuickActionQuery(
  inputQuery: string,
  seed: MainPanelClipboardSeed | null
): string {
  const trimmed = inputQuery.trim();
  if (seed?.kind === "text" && seed.fullText) {
    return seed.fullText.trim();
  }
  return trimmed;
}

export function resolveQuickActionInput(
  inputQuery: string,
  seed: MainPanelClipboardSeed | null
): QuickActionInput {
  if (seed?.kind === "image" && seed.entryId != null && seed.imageUrl) {
    return {
      kind: "image",
      entryId: seed.entryId,
      imageUrl: seed.imageUrl,
      width: seed.imageWidth,
      height: seed.imageHeight,
    };
  }
  if (seed?.kind === "file" && seed.entryId != null && seed.paths && seed.paths.length > 0) {
    return {
      kind: "file",
      entryId: seed.entryId,
      paths: seed.paths,
    };
  }
  if (seed?.kind === "text" && seed.fullText?.trim()) {
    return { kind: "text", text: seed.fullText.trim() };
  }
  const text = inputQuery.trim();
  return text ? { kind: "text", text } : { kind: "none" };
}

export type MainPanelClipboardChip =
  | { kind: "text"; fullText: string; label: string }
  | {
      kind: "image";
      entryId: number;
      imageUrl: string;
      imageWidth?: number | null;
      imageHeight?: number | null;
    }
  | {
      kind: "file";
      entryId: number;
      paths: string[];
      label: string;
    };

export function seedToMainPanelChip(
  seed: MainPanelClipboardSeed
): MainPanelClipboardChip | null {
  if (seed.kind === "text" && seed.fullText) {
    return {
      kind: "text",
      fullText: seed.fullText,
      label: truncateClipboardChipLabel(seed.fullText),
    };
  }
  if (seed.kind === "image" && seed.entryId != null && seed.imageUrl) {
    return {
      kind: "image",
      entryId: seed.entryId,
      imageUrl: seed.imageUrl,
      imageWidth: seed.imageWidth,
      imageHeight: seed.imageHeight,
    };
  }
  if (seed.kind === "file" && seed.entryId != null) {
    const paths =
      seed.paths && seed.paths.length > 0
        ? seed.paths
        : [];
    if (paths.length === 0) return null;
    return {
      kind: "file",
      entryId: seed.entryId,
      paths,
      label: truncateClipboardChipLabel(formatClipboardFilesPreview(paths)),
    };
  }
  return null;
}
