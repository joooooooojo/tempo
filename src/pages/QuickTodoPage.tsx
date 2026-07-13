import { useCallback, useEffect, useRef, useState, type SyntheticEvent } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "@/lib/api";
import { applyTheme, subscribeThemeChanges } from "@/lib/theme";
import { isMacTarget, isWindowsTarget } from "@/lib/utils";
import type { TodoItem } from "@/types";

export function QuickTodoPage() {
  const [title, setTitle] = useState("");
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const savingRef = useRef(false);

  const focusTitleInput = useCallback(() => {
    setTitle("");
    setSaving(false);
    window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, []);

  useEffect(() => {
    const previousBodyOverflow = document.body.style.overflow;
    const root = document.documentElement;
    const platformClass = isMacTarget
      ? "quick-todo-window--mac"
      : isWindowsTarget
        ? "quick-todo-window--windows"
        : "quick-todo-window--css-shadow";
    root.classList.add("quick-todo-window");
    root.classList.add(platformClass);
    document.body.classList.add("quick-todo-window");
    document.body.style.overflow = "hidden";
    void applyThemeFromSettings();

    const unsubscribeTheme = subscribeThemeChanges((theme) => {
      applyTheme(theme);
    });

    return () => {
      root.classList.remove(
        "quick-todo-window",
        "quick-todo-window--mac",
        "quick-todo-window--windows",
        "quick-todo-window--css-shadow"
      );
      document.body.classList.remove("quick-todo-window");
      document.body.style.overflow = previousBodyOverflow;
      unsubscribeTheme();
    };
  }, []);

  useEffect(() => {
    savingRef.current = saving;
  }, [saving]);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let armed = false;
    let armTimer = 0;

    const armBlurClose = () => {
      window.clearTimeout(armTimer);
      armTimer = window.setTimeout(() => {
        armed = true;
      }, 200);
    };

    const unlistenFocus = listen("quick-todo:focus-title", () => {
      focusTitleInput();
      armBlurClose();
    });

    let unlistenBlur: (() => void) | undefined;
    void appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused && armed && !savingRef.current) {
          void hideWindow();
        }
      })
      .then((fn) => {
        unlistenBlur = fn;
      });

    return () => {
      window.clearTimeout(armTimer);
      void unlistenFocus.then((fn) => fn());
      unlistenBlur?.();
    };
  }, [focusTitleInput]);

  const submit = async (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();

    const nextTitle = title.trim();
    if (!nextTitle) {
      focusTitleInput();
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(nextTitle, "", null);
      await emit<TodoItem>("todo-created", created);
      window.setTimeout(() => void hideWindow(), 100);
    } catch {
      setSaving(false);
    }
  };

  const close = () => {
    if (!saving) void hideWindow();
  };

  return (
    <div className="quick-todo-page">
      <form className="quick-todo-panel text-foreground" onSubmit={submit}>
        <input
          ref={inputRef}
          value={title}
          maxLength={120}
          placeholder="输入待办事项标题"
          disabled={saving}
          className="quick-todo-input"
          onChange={(event) => setTitle(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") close();
          }}
        />
      </form>
    </div>
  );
}

async function applyThemeFromSettings() {
  try {
    const settings = await api.getSettings();
    applyTheme(settings.theme);
  } catch {
    applyTheme("system");
  }
}

async function hideWindow() {
  await getCurrentWindow().hide();
}
