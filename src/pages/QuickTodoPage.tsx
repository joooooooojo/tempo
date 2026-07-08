import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { TodoItem } from "@/types";

export function QuickTodoPage() {
  const [title, setTitle] = useState("");
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const previousBodyOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    api.getSettings().then((settings) => {
      const root = document.documentElement;
      if (settings.theme === "dark") root.classList.add("dark");
      else if (settings.theme === "light") root.classList.remove("dark");
      else {
        root.classList.toggle("dark", window.matchMedia("(prefers-color-scheme: dark)").matches);
      }
    });

    return () => {
      document.body.style.overflow = previousBodyOverflow;
    };
  }, []);

  useEffect(() => {
    focusTitleInput();
    const unlisten = listen("quick-todo:focus-title", focusTitleInput);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

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
      window.setTimeout(() => void closeWindow(), 100);
    } catch (error) {
      setSaving(false);
    }
  };

  const close = () => {
    if (!saving) void closeWindow();
  };

  const focusTitleInput = () => {
    window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  };

  return (
    <div className="quick-todo-page text-foreground">
        <form className="flex min-h-0 flex-1 items-center px-4 py-4" onSubmit={submit}>
          <input
            ref={inputRef}
            value={title}
            maxLength={120}
            placeholder="输入待办标题"
            disabled={saving}
            className={cn(
              "h-12 w-full rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[15px] font-medium text-foreground shadow-sm shadow-emerald-950/[0.03] outline-none transition-colors placeholder:text-muted-foreground",
              "focus:border-primary/45 focus:ring-2 focus:ring-primary/20 disabled:cursor-not-allowed disabled:opacity-60"
            )}
            onChange={(event) => setTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") close();
            }}
          />
        </form>
    </div>
  );
}

async function closeWindow() {
  await getCurrentWindow().close();
}
