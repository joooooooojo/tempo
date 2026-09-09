const STORAGE_KEY = "tempo:tag-history";
const MAX_HISTORY = 50;

export function getTagHistory(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is string => typeof item === "string" && item.trim().length > 0);
  } catch {
    return [];
  }
}

export function recordTag(tag: string) {
  const normalized = tag.trim();
  if (!normalized) return;

  const key = normalized.toLocaleLowerCase();
  const history = getTagHistory().filter((item) => item.toLocaleLowerCase() !== key);
  localStorage.setItem(STORAGE_KEY, JSON.stringify([normalized, ...history].slice(0, MAX_HISTORY)));
}

export function mergeTagSuggestions(history: string[], ...sources: string[][]): string[] {
  const seen = new Set<string>();
  const merged: string[] = [];

  const append = (tags: string[], sort = false) => {
    const bucket: string[] = [];
    for (const tag of tags) {
      const normalized = tag.trim();
      if (!normalized) continue;
      const key = normalized.toLocaleLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      bucket.push(normalized);
    }
    if (sort) {
      bucket.sort((a, b) => a.localeCompare(b, "zh-CN"));
    }
    merged.push(...bucket);
  };

  append(history, false);
  for (const source of sources) {
    append(source, true);
  }
  return merged;
}
