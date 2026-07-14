import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { cn } from "@/lib/utils";
import { highlightCode } from "@/components/CodeHighlight";

/** Editable content area. Plain text uses textarea; code language uses one contenteditable div. */
export function CodeEditor({
  id,
  value,
  language,
  placeholder,
  className,
  onChange,
}: {
  id?: string;
  value: string;
  language?: string | null;
  placeholder?: string;
  className?: string;
  onChange: (value: string) => void;
}) {
  const editorRef = useRef<HTMLDivElement>(null);
  const skipSyncRef = useRef(false);
  const composingRef = useRef(false);
  const appliedHtmlRef = useRef<string | null>(null);
  const [composing, setComposing] = useState(false);
  const isPlain = !language || language === "plain";
  const html = useMemo(() => highlightCode(value || "", language), [value, language]);

  useEffect(() => {
    if (isPlain) {
      appliedHtmlRef.current = null;
      return;
    }
    // Rewriting DOM during IME composition commits raw pinyin and breaks spelling.
    if (composingRef.current) return;

    const el = editorRef.current;
    if (!el) return;

    const applyHtml = () => {
      const hadFocus = document.activeElement === el;
      const offset = hadFocus ? getCaretCharacterOffset(el) : 0;
      el.innerHTML = html || "<br>";
      appliedHtmlRef.current = html;
      if (hadFocus) setCaretCharacterOffset(el, offset);
    };

    if (skipSyncRef.current) {
      skipSyncRef.current = false;
      const current = normalizeEditableText(el.innerText);
      if (current === value) {
        applyHtml();
        return;
      }
    }

    // Text may be unchanged when language switches — still re-apply highlight HTML.
    if (
      normalizeEditableText(el.innerText) === value &&
      el.childNodes.length > 0 &&
      appliedHtmlRef.current === html
    ) {
      return;
    }

    applyHtml();
  }, [html, isPlain, value]);

  const emitChange = (el: HTMLDivElement) => {
    skipSyncRef.current = true;
    onChange(normalizeEditableText(el.innerText));
  };

  if (isPlain) {
    return (
      <textarea
        id={id}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          "code-editor__plain flex h-full min-h-0 w-full flex-1 resize-none rounded-lg border border-input bg-background px-3 py-2",
          "placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          className
        )}
      />
    );
  }

  return (
    <div
      className={cn(
        "code-editor relative h-full min-h-0 w-full flex-1 overflow-auto rounded-lg border border-input bg-background",
        "focus-within:ring-2 focus-within:ring-ring",
        className
      )}
    >
      {!value && !composing && placeholder && (
        <div className="code-editor__placeholder pointer-events-none absolute left-3 top-2 text-muted-foreground">
          {placeholder}
        </div>
      )}
      <div
        id={id}
        ref={editorRef}
        role="textbox"
        aria-multiline="true"
        contentEditable
        suppressContentEditableWarning
        spellCheck={false}
        className={cn(
          "code-editor__input min-h-40 w-full whitespace-pre-wrap break-words px-3 py-2 text-foreground outline-none",
          "[&_.hljs]:bg-transparent! [&_.hljs]:p-0!"
        )}
        onCompositionStart={() => {
          composingRef.current = true;
          setComposing(true);
        }}
        onCompositionEnd={(event) => {
          composingRef.current = false;
          setComposing(false);
          emitChange(event.currentTarget);
        }}
        onInput={(event: FormEvent<HTMLDivElement>) => {
          const native = event.nativeEvent as InputEvent;
          if (composingRef.current || native.isComposing) return;
          emitChange(event.currentTarget);
        }}
        onKeyDown={(event) => {
          if (composingRef.current) return;
          if (event.key !== "Tab" || event.metaKey || event.ctrlKey || event.altKey) return;
          event.preventDefault();
          document.execCommand("insertText", false, "  ");
        }}
      />
    </div>
  );
}

function normalizeEditableText(value: string) {
  return value.replace(/\u00a0/g, " ").replace(/\n$/, "");
}

function getCaretCharacterOffset(element: HTMLElement) {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return 0;
  const range = selection.getRangeAt(0);
  const preCaret = range.cloneRange();
  preCaret.selectNodeContents(element);
  preCaret.setEnd(range.endContainer, range.endOffset);
  return preCaret.toString().length;
}

function setCaretCharacterOffset(element: HTMLElement, offset: number) {
  const selection = window.getSelection();
  if (!selection) return;

  let remaining = Math.max(0, offset);
  const nodeIterator = document.createNodeIterator(element, NodeFilter.SHOW_TEXT);
  let textNode = nodeIterator.nextNode();

  while (textNode) {
    const length = textNode.textContent?.length ?? 0;
    if (remaining <= length) {
      const range = document.createRange();
      range.setStart(textNode, remaining);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      return;
    }
    remaining -= length;
    textNode = nodeIterator.nextNode();
  }

  const range = document.createRange();
  range.selectNodeContents(element);
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
}
