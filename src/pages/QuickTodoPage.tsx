import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast, Toaster } from "sonner";
import {
  TodoCreateFormPanel,
  todoDateTimeLocalToIso,
  type DraftTodoImage,
} from "@/components/todos/TodoCreateDialog";
import { api, type TodoImageInput } from "@/lib/api";
import type { TodoItem } from "@/types";

type DraftImage = DraftTodoImage;

interface QuickClipboardContent {
  title: string;
  images: DraftImage[];
}

const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

export function QuickTodoPage() {
  const [title, setTitle] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [images, setImages] = useState<DraftImage[]>([]);
  const [reading, setReading] = useState(true);
  const [saving, setSaving] = useState(false);
  const readRequest = useRef(0);

  useEffect(() => {
    const previousHtmlBackground = document.documentElement.style.background;
    const previousBodyBackground = document.body.style.background;
    const previousBodyOverflow = document.body.style.overflow;
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
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
      document.documentElement.style.background = previousHtmlBackground;
      document.body.style.background = previousBodyBackground;
      document.body.style.overflow = previousBodyOverflow;
    };
  }, []);

  const readClipboard = useCallback(async () => {
    const request = readRequest.current + 1;
    readRequest.current = request;
    setReading(true);
    setTitle("");
    setDueAt("");
    setImages([]);

    try {
      const content = await readFirstClipboardTodoContent();
      if (readRequest.current !== request) return;

      setTitle(content.title);
      setImages(content.images);
      if (!content.title && content.images.length === 0) {
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
    if (!title.trim() && images.length === 0) {
      toast.error("请输入待办内容");
      return;
    }

    setSaving(true);
    try {
      const created = await api.addTodo(
        title,
        todoDateTimeLocalToIso(dueAt),
        images.map(toTodoImageInput)
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
    <div className="quick-todo-page min-h-screen bg-transparent text-foreground">
      <div className="todo-create-dialog quick-todo-panel flex min-h-screen w-screen flex-col overflow-visible border border-border/80">
        <TodoCreateFormPanel
          layout="window"
          heading="快速新建待办"
          todoTitle={title}
          dueAt={dueAt}
          images={images}
          saving={reading || saving}
          titlePlaceholder={reading ? "正在读取剪贴板" : "待办内容"}
          onCancel={() => handleOpenChange(false)}
          onTitleChange={setTitle}
          onDueAtChange={setDueAt}
          onDeleteImage={(image) =>
            setImages((current) => current.filter((item) => item.local_id !== image.local_id))
          }
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
          return { title: normalizeClipboardTodoTitle(await blob.text()), images: [] };
        }

        const imageType = firstItem.types.find((type) => type.startsWith("image/"));
        if (imageType) {
          const blob = await firstItem.getType(imageType);
          const image = await createDraftImageFromBlob(blob);
          return { title: "", images: image ? [image] : [] };
        }
      }
    } catch {
      // Some WebViews only expose text clipboard reads.
    }
  }

  const text = await clipboard.readText();
  return { title: normalizeClipboardTodoTitle(text), images: [] };
}

function normalizeClipboardTodoTitle(value: string) {
  const normalized = value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .join("\n")
    .trim();
  const chars = Array.from(normalized);
  if (chars.length <= 120) return normalized;

  toast.info("剪贴板内容已截断到 120 个字");
  return chars.slice(0, 120).join("");
}

async function createDraftImageFromBlob(blob: Blob) {
  if (blob.size > MAX_IMAGE_BYTES) {
    toast.error("单张图片不能超过 5MB");
    return null;
  }

  if (!["image/png", "image/jpeg", "image/webp", "image/gif"].includes(blob.type)) {
    toast.error("仅支持 PNG、JPEG、WebP 或 GIF 图片");
    return null;
  }

  return {
    local_id: createLocalId(),
    data_url: await readFileAsDataUrl(blob),
    mime_type: blob.type,
  };
}

function readFileAsDataUrl(file: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("图片读取失败"));
    reader.readAsDataURL(file);
  });
}

function toTodoImageInput(image: DraftImage): TodoImageInput {
  return {
    data_url: image.data_url,
    mime_type: image.mime_type,
  };
}

function createLocalId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败";
}
