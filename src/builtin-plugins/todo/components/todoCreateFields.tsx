import {
  useEffect,
  useRef,
  useState,
  type ChangeEventHandler,
  type ClipboardEvent,
} from "react";
import { cn } from "@/lib/utils";

const FLOATING_LABEL_VARIANTS = {
  input: "todo-floating-label--input",
  textarea: "todo-floating-label--textarea",
} as const;

export function FloatingFieldLabel({
  placeholder,
  floated,
  focused,
  variant,
}: {
  placeholder: string;
  floated: boolean;
  focused: boolean;
  variant: keyof typeof FLOATING_LABEL_VARIANTS;
}) {
  return (
    <span
      data-floated={floated || undefined}
      className={cn(
        "todo-floating-label",
        FLOATING_LABEL_VARIANTS[variant],
        floated && focused && "todo-floating-label--focused"
      )}
    >
      {placeholder}
    </span>
  );
}

export function FloatingInput({
  id,
  value,
  placeholder,
  maxLength,
  autoFocus,
  required,
  className,
  onChange,
}: {
  id: string;
  value: string;
  placeholder: string;
  maxLength?: number;
  autoFocus?: boolean;
  required?: boolean;
  className?: string;
  onChange: ChangeEventHandler<HTMLInputElement>;
}) {
  const [focused, setFocused] = useState(false);
  // autoFocus fields must start editable — toggling readOnly on first key breaks IME.
  const [autofillBlocked, setAutofillBlocked] = useState(!autoFocus);
  const inputRef = useRef<HTMLInputElement>(null);
  const floated = focused || value.length > 0;

  const releaseAutofillBlock = () => {
    setAutofillBlocked((blocked) => (blocked ? false : blocked));
  };

  useEffect(() => {
    // Only re-arm the autofill shield when the field is empty AND blurred.
    // Re-applying readOnly while focused breaks IME on the first keystroke.
    if (value || focused || autoFocus) return;
    setAutofillBlocked(true);
  }, [value, focused, autoFocus]);

  useEffect(() => {
    if (!autoFocus) return;

    const frame = requestAnimationFrame(() => {
      const input = inputRef.current;
      if (!input) return;

      setAutofillBlocked(false);
      const end = input.value.length;
      input.focus();
      input.setSelectionRange(end, end);
    });

    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);

  return (
    <div className="relative">
      <input
        ref={inputRef}
        id={id}
        name={`tempo-${id}`}
        autoComplete="off"
        autoFocus={autoFocus}
        required={required}
        readOnly={autofillBlocked}
        value={value}
        maxLength={maxLength}
        placeholder=""
        className={cn(
          "block h-11 w-full rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 text-[14px] font-semibold leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50",
          floated && "border-primary/45",
          className
        )}
        onChange={onChange}
        onMouseDown={releaseAutofillBlock}
        onFocus={() => {
          releaseAutofillBlock();
          setFocused(true);
        }}
        onBlur={() => setFocused(false)}
      />
      <FloatingFieldLabel
        placeholder={placeholder}
        floated={floated}
        focused={focused}
        variant="input"
      />
    </div>
  );
}

export function FloatingTextarea({
  id,
  value,
  placeholder,
  maxLength,
  autoFocus,
  className,
  onChange,
  onPaste,
}: {
  id: string;
  value: string;
  placeholder: string;
  maxLength?: number;
  autoFocus?: boolean;
  className?: string;
  onChange: ChangeEventHandler<HTMLTextAreaElement>;
  onPaste?: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
}) {
  const [focused, setFocused] = useState(false);
  const [autofillBlocked, setAutofillBlocked] = useState(!autoFocus);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const floated = focused || value.length > 0;

  const releaseAutofillBlock = () => {
    setAutofillBlocked((blocked) => (blocked ? false : blocked));
  };

  useEffect(() => {
    if (value || focused || autoFocus) return;
    setAutofillBlocked(true);
  }, [value, focused, autoFocus]);

  useEffect(() => {
    if (!autoFocus) return;

    const frame = requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;

      setAutofillBlocked(false);
      const end = textarea.value.length;
      textarea.focus();
      textarea.setSelectionRange(end, end);
    });

    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        id={id}
        name={`tempo-${id}`}
        autoComplete="off"
        autoFocus={autoFocus}
        readOnly={autofillBlocked}
        value={value}
        maxLength={maxLength}
        placeholder=""
        className={cn(
          "block min-h-20 w-full resize-none rounded-lg border border-border/70 bg-[var(--todo-field-bg)] px-3 py-3 text-[14px] leading-5 shadow-sm shadow-emerald-950/[0.03] transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50",
          floated && "border-primary/45",
          className
        )}
        onChange={onChange}
        onPaste={onPaste}
        onMouseDown={releaseAutofillBlock}
        onFocus={() => {
          releaseAutofillBlock();
          setFocused(true);
        }}
        onBlur={() => setFocused(false)}
      />
      <FloatingFieldLabel
        placeholder={placeholder}
        floated={floated}
        focused={focused}
        variant="textarea"
      />
    </div>
  );
}
