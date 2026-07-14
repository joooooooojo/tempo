/** Convert a KeyboardEvent into a global-hotkey style shortcut string (e.g. `F2`, `Control+Shift+V`). */
export function shortcutFromKeyboardEvent(event: KeyboardEvent): string | null {
  const key = event.key;
  if (!key || key === "Shift" || key === "Control" || key === "Alt" || key === "Meta") {
    return null;
  }
  if (key === "Escape") {
    return null;
  }

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");

  let main = key;
  if (/^F\d{1,2}$/i.test(key)) {
    main = key.toUpperCase();
  } else if (key.length === 1) {
    main = key.toUpperCase();
  } else if (key === " ") {
    main = "Space";
  } else if (key === "ArrowUp") {
    main = "Up";
  } else if (key === "ArrowDown") {
    main = "Down";
  } else if (key === "ArrowLeft") {
    main = "Left";
  } else if (key === "ArrowRight") {
    main = "Right";
  } else {
    main = key;
  }

  // Bare letter/digit without modifiers is too easy to conflict — require F-keys or modifiers
  const isFunctionKey = /^F\d{1,2}$/i.test(main);
  if (!isFunctionKey && parts.length === 0) {
    return null;
  }

  parts.push(main);
  return parts.join("+");
}

export function formatShortcutLabel(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => {
      const lower = part.toLowerCase();
      if (lower === "control" || lower === "ctrl") return "Ctrl";
      if (lower === "super" || lower === "cmd" || lower === "command") return "⌘";
      if (lower === "alt" || lower === "option") return "Alt";
      if (lower === "shift") return "Shift";
      return part.toUpperCase() === part ? part : part;
    })
    .join(" + ");
}

export const DEFAULT_SHORTCUTS = {
  shortcut_quick_todo: "F2",
  shortcut_clipboard_picker: "F4",
  shortcut_snippet_picker: "F5",
} as const;
