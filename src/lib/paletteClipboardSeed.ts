import type { CommandPaletteClipboardSeed } from "@/types";

/** Short text goes straight into the search input; longer text uses a leading chip. */
export const PALETTE_CLIPBOARD_INLINE_MAX_LEN = 48;

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
  return trimmed.length <= PALETTE_CLIPBOARD_INLINE_MAX_LEN;
}

export function resolveQuickActionQuery(
  inputQuery: string,
  seed: CommandPaletteClipboardSeed | null
): string {
  const trimmed = inputQuery.trim();
  if (seed?.kind === "text" && seed.fullText) {
    return trimmed || seed.fullText.trim();
  }
  return trimmed;
}

export type PaletteClipboardChip =
  | { kind: "text"; fullText: string; label: string }
  | {
      kind: "image";
      entryId: number;
      imageUrl: string;
      imageWidth?: number | null;
      imageHeight?: number | null;
    };

export function seedToPaletteChip(
  seed: CommandPaletteClipboardSeed
): PaletteClipboardChip | null {
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
  return null;
}
