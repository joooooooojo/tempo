import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Pencil, Plus, Search, TextQuote, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { cn, formatRelativeTime, previewLines } from "@/lib/utils";
import type { Snippet } from "@/types";

type EditorState = {
  id?: number;
  title: string;
  content: string;
  tags: string;
};

const emptyEditor: EditorState = { title: "", content: "", tags: "" };

export function SnippetsPage() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [query, setQuery] = useState("");
  const [editorOpen, setEditorOpen] = useState(false);
  const [editor, setEditor] = useState<EditorState>(emptyEditor);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    setSnippets(await api.getSnippets(query || undefined));
  }, [query]);

  useEffect(() => {
    void load();
    const unlisten = listen("snippets-update", () => void load());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 200);
    return () => window.clearTimeout(timer);
  }, [load, query]);

  const openCreate = () => {
    setEditor(emptyEditor);
    setEditorOpen(true);
  };

  const openEdit = (snippet: Snippet) => {
    setEditor({
      id: snippet.id,
      title: snippet.title,
      content: snippet.content,
      tags: snippet.tags.join(", "),
    });
    setEditorOpen(true);
  };

  const saveEditor = async () => {
    const title = editor.title.trim();
    const content = editor.content.trim();
    const tags = editor.tags
      .split(/[,，]/)
      .map((tag) => tag.trim())
      .filter(Boolean);
    if (!title || !content) {
      toast.error("请填写标题和内容");
      return;
    }

    setSaving(true);
    try {
      if (editor.id) {
        await api.updateSnippet(editor.id, title, content, tags);
      } else {
        await api.createSnippet(title, content, tags);
      }
      setEditorOpen(false);
      void load();
      toast.success("已保存");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "保存失败");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 text-xl font-bold tracking-tight">
            <TextQuote className="h-5 w-5 text-primary" />
            快捷短语
          </h1>
          <p className="mt-1 text-[13px] text-muted-foreground">
            保存常用文字，一键复制。按 <kbd className="rounded bg-foreground/8 px-1.5 py-0.5 text-[11px]">F5</kbd> 快速呼出
          </p>
        </div>
        <Button className="h-9" onClick={openCreate}>
          <Plus className="mr-1.5 h-4 w-4" />
          新建短语
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索短语..."
            className="h-9 border-0 pl-9 glass-subtle"
          />
        </div>
        <Button variant="outline" className="h-9 border-0 glass-subtle" onClick={() => void api.showSnippetPicker()}>
          打开浮层 (F5)
        </Button>
      </div>

      <div className="space-y-2">
        {snippets.length === 0 ? (
          <Card className="border-dashed">
            <CardContent className="py-10 text-center text-[13px] text-muted-foreground">
              还没有快捷短语，点击「新建短语」开始添加
            </CardContent>
          </Card>
        ) : (
          snippets.map((snippet) => (
            <Card key={snippet.id} className="overflow-hidden transition-colors hover:bg-foreground/[0.03]">
              <CardContent className="p-0">
                <div className="flex items-center justify-between gap-3 border-b border-border/40 bg-sky-500/10 px-3 py-2 text-[11px]">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="font-semibold text-sky-600 dark:text-sky-300">短语</span>
                    <span className="truncate font-medium text-foreground">{snippet.title}</span>
                  </div>
                  <span className="text-muted-foreground">{formatRelativeTime(snippet.updated_at)}</span>
                </div>
                <div className="flex items-start justify-between gap-3 px-3 py-3">
                  <div className="min-w-0 flex-1">
                    <pre className="whitespace-pre-wrap break-words font-sans text-[13px] leading-relaxed text-foreground/90">
                      {previewLines(snippet.content, 6)}
                    </pre>
                    {snippet.tags.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1.5">
                        {snippet.tags.map((tag) => (
                          <span
                            key={tag}
                            className="rounded-full bg-foreground/6 px-2 py-0.5 text-[11px] text-muted-foreground"
                          >
                            {tag}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                  <div className="flex shrink-0 gap-1">
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8"
                      title="复制"
                      onClick={() =>
                        void api.copySnippetToClipboard(snippet.id).then(() => toast.success("已复制"))
                      }
                    >
                      <TextQuote className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8"
                      title="编辑"
                      onClick={() => openEdit(snippet)}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8 text-destructive"
                      title="删除"
                      onClick={() => void api.deleteSnippet(snippet.id).then(() => load())}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))
        )}
      </div>

      <Dialog open={editorOpen} onOpenChange={setEditorOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{editor.id ? "编辑短语" : "新建短语"}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="snippet-title">标题</Label>
              <Input
                id="snippet-title"
                value={editor.title}
                onChange={(event) => setEditor((state) => ({ ...state, title: event.target.value }))}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="snippet-content">内容</Label>
              <textarea
                id="snippet-content"
                rows={8}
                value={editor.content}
                onChange={(event) => setEditor((state) => ({ ...state, content: event.target.value }))}
                className={cn(
                  "flex min-h-[160px] w-full rounded-lg border border-input bg-background px-3 py-2 text-sm",
                  "ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                )}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="snippet-tags">标签（逗号分隔）</Label>
              <Input
                id="snippet-tags"
                value={editor.tags}
                onChange={(event) => setEditor((state) => ({ ...state, tags: event.target.value }))}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditorOpen(false)}>
              取消
            </Button>
            <Button disabled={saving} onClick={() => void saveEditor()}>
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
