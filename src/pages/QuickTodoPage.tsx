import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast, Toaster } from "sonner";
import {
  TodoCreateFormPanel,
  todoDateTimeLocalToIso,
} from "@/components/todos/TodoCreateDialog";
import { api } from "@/lib/api";
import { markdownImageFromBlob } from "@/lib/markdownImages";
import type { TodoItem } from "@/types";

interface QuickClipboardContent {
  title: string;
  content: string;
}

export function QuickTodoPage() {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [reading, setReading] = useState(true);
  const [saving, setSaving] = useState(false);
  const readRequest = useRef(0);

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

  const readClipboard = useCallback(async () => {
    const request = readRequest.current + 1;
    readRequest.current = request;
    setReading(true);
    setTitle("");
    setContent("");
    setDueAt("");

    try {
      const content = await readFirstClipboardTodoContent();
      if (readRequest.current !== request) return;

      setTitle(content.title);
      setContent(content.content);
      if (!content.title && !content.content) {
        toast.info("剪贴板没有可用的文字或图片");
      }
    } catch (error) {
      if (readRequest.current !== request) return;
      toast.error(errorMessage(error));
    } finally {
      if (readRequest.current === request) {
        setReading(false);
      }
    }
  }, []);

  useEffect(() => {
    void readClipboard();
    const unlisten = listen("quick-todo:read-clipboard", () => void readClipboard());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [readClipboard]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!title.trim() && !content.trim()) {
      toast.error("请输入待办标题或内容");
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(
        title,
        content,
        todoDateTimeLocalToIso(dueAt)
      );
      await emit<TodoItem>("todo-created", created);
      toast.success("已创建");
      window.setTimeout(() => void closeWindow(), 120);
    } catch (error) {
      toast.error(errorMessage(error));
      setSaving(false);
    }
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen && !saving) {
      void closeWindow();
    }
  };

  return (
    <div className="quick-todo-page min-h-screen text-foreground">
      <div className="todo-create-dialog quick-todo-panel flex min-h-screen w-screen flex-col overflow-visible border border-border/80">
        <TodoCreateFormPanel
          layout="window"
          heading="快速新建待办"
          todoTitle={title}
          todoContent={content}
          dueAt={dueAt}
          saving={reading || saving}
          titlePlaceholder={reading ? "正在读取剪贴板" : "待办标题"}
          contentPlaceholder="待办内容（支持 Markdown，粘贴图片会嵌入正文）"
          onCancel={() => handleOpenChange(false)}
          onTitleChange={setTitle}
          onContentChange={setContent}
          onDueAtChange={setDueAt}
          onSubmit={submit}
        />
      </div>

      <Toaster position="top-center" richColors toastOptions={{ className: "glass rounded-lg" }} />
    </div>
  );
}

async function closeWindow() {
  await getCurrentWindow().close();
}

async function readFirstClipboardTodoContent(): Promise<QuickClipboardContent> {
  const clipboard = navigator.clipboard;
  if (!clipboard) {
    throw new Error("当前环境无法读取剪贴板");
  }

  const readableClipboard = clipboard as Clipboard & {
    read?: () => Promise<ClipboardItem[]>;
  };

  if (readableClipboard.read) {
    try {
      const [firstItem] = await readableClipboard.read();
      if (firstItem) {
        const textType = firstItem.types.find((type) => type === "text/plain")
          ?? firstItem.types.find((type) => type.startsWith("text/"));
        if (textType) {
          const blob = await firstItem.getType(textType);
          return normalizeClipboardTodoText(await blob.text());
        }

        const imageType = firstItem.types.find((type) => type.startsWith("image/"));
        if (imageType) {
          const blob = await firstItem.getType(imageType);
          return { title: "图片待办", content: await markdownImageFromBlob(blob) };
        }
      }
    } catch {
      // Some WebViews only expose text clipboard reads.
    }
  }

  const text = await clipboard.readText();
  return normalizeClipboardTodoText(text);
}

function normalizeClipboardTodoText(value: string): QuickClipboardContent {
  const content = value
    .replace(/\r\n?/g, "\n")
    .trim();
  return { title: deriveClipboardTodoTitle(content), content };
}

function deriveClipboardTodoTitle(value: string) {
  const normalized = value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) ?? "";
  const plain = normalized
    .replace(/^#{1,6}\s+/, "")
    .replace(/^[-*+>]\s*/, "")
    .replace(/\*\*/g, "")
    .replace(/__/g, "")
    .replace(/`/g, "")
    .trim();
  const chars = Array.from(plain);
  if (chars.length <= 120) return plain;

  toast.info("待办标题已按 120 个字自动截断");
  return chars.slice(0, 120).join("");
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败";
}
