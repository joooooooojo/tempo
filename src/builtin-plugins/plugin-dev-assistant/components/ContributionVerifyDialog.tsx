import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { FolderOpen, Play, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { messageOf } from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import type {
  EditablePluginCommand,
  EditablePluginMcpTool,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import type { PluginDevLogEvent } from "@/types";

export type ContributionVerifyContext = {
  projectId: string;
  pluginId: string | null;
  connected: boolean;
};

export type ContributionVerifyTarget =
  | { kind: "command"; item: EditablePluginCommand }
  | { kind: "mcp"; item: EditablePluginMcpTool };

type CommandInputKind = "text" | "image" | "file";

const COMMAND_INPUT_KIND_ITEMS = [
  { value: "text", label: "字符串" },
  { value: "image", label: "图片" },
  { value: "file", label: "文件" },
] as const;

function formatLogs(logs: PluginDevLogEvent[]) {
  return logs.length > 0
    ? logs.map((entry) => `[${entry.source}] ${entry.message}`).join("\n")
    : "暂无日志";
}

function targetTitle(target: ContributionVerifyTarget) {
  switch (target.kind) {
    case "command":
      return target.item.title || target.item.id || "Command";
    case "mcp":
      return target.item.name;
  }
}

function buildCommandVerifyParams(
  commandId: string,
  kind: CommandInputKind,
  text: string,
  imagePath: string | null,
  filePaths: string[],
): Record<string, unknown> | null {
  const actionId = `dev-verify:${commandId}`;
  switch (kind) {
    case "text": {
      const value = text.trim();
      return {
        actionId,
        query: value,
        input: { kind: "text", text: value },
      };
    }
    case "image": {
      if (!imagePath) return null;
      return {
        actionId,
        query: "",
        input: {
          kind: "image",
          entryId: 0,
          imageUrl: "",
          filePath: imagePath,
        },
      };
    }
    case "file": {
      if (filePaths.length === 0) return null;
      return {
        actionId,
        query: "",
        input: {
          kind: "file",
          entryId: 0,
          paths: filePaths,
        },
      };
    }
  }
}

function emptyValueForJsonSchemaType(type: unknown): unknown {
  const resolved =
    Array.isArray(type)
      ? (type.find((item) => item !== "null") ?? type[0])
      : type;
  switch (resolved) {
    case "string":
      return "";
    case "number":
    case "integer":
      return 0;
    case "boolean":
      return false;
    case "array":
      return [];
    case "object":
      return {};
    case "null":
      return null;
    default:
      return null;
  }
}

function emptyValueForSchemaProperty(raw: unknown): unknown {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const property = raw as Record<string, unknown>;
  if ("default" in property) return property.default;
  if (Array.isArray(property.enum) && property.enum.length > 0) {
    return property.enum[0];
  }
  return emptyValueForJsonSchemaType(property.type);
}

/** Build a starter arguments object from MCP `inputSchema.properties`. */
function buildMcpArgumentsStub(
  inputSchema: Record<string, unknown> | null | undefined,
): Record<string, unknown> {
  const properties =
    inputSchema?.properties &&
    typeof inputSchema.properties === "object" &&
    !Array.isArray(inputSchema.properties)
      ? (inputSchema.properties as Record<string, unknown>)
      : {};
  const stub: Record<string, unknown> = {};
  for (const [name, raw] of Object.entries(properties)) {
    stub[name] = emptyValueForSchemaProperty(raw);
  }
  return stub;
}

function serializeMcpArguments(value: Record<string, unknown>) {
  return JSON.stringify(value, null, 2);
}

function parseMcpArgumentsJson(
  raw: string,
): { ok: true; value: Record<string, unknown> } | { ok: false; error: string } {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { ok: false, error: "必须是 JSON 对象" };
    }
    return { ok: true, value: parsed as Record<string, unknown> };
  } catch (cause) {
    return {
      ok: false,
      error: cause instanceof Error ? cause.message : String(cause),
    };
  }
}

function useDialogLogs(pluginId: string | null, open: boolean) {
  const [logs, setLogs] = useState<PluginDevLogEvent[]>([]);

  useEffect(() => {
    if (!open || !pluginId) {
      setLogs([]);
      return;
    }
    setLogs([]);
    const unlistenLog = listen<PluginDevLogEvent>("plugin-dev://log", (event) => {
      if (event.payload.pluginId !== pluginId) return;
      setLogs((current) => [...current.slice(-199), event.payload]);
    });
    const unlistenState = listen<{
      pluginId: string;
      state: string;
      message?: string | null;
    }>("plugin-dev://runtime-state", (event) => {
      if (event.payload.pluginId !== pluginId) return;
      setLogs((current) => [
        ...current.slice(-199),
        {
          pluginId: event.payload.pluginId,
          source: "host",
          message: event.payload.message
            ? `${event.payload.state}: ${event.payload.message}`
            : event.payload.state,
          at: new Date().toISOString(),
        },
      ]);
    });
    return () => {
      void unlistenLog.then((dispose) => dispose());
      void unlistenState.then((dispose) => dispose());
    };
  }, [open, pluginId]);

  return { logs, setLogs };
}

export function ContributionVerifyDialog({
  target,
  context,
}: {
  target: ContributionVerifyTarget;
  context: ContributionVerifyContext;
}) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [inputKind, setInputKind] = useState<CommandInputKind>("text");
  const [textInput, setTextInput] = useState("");
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [filePaths, setFilePaths] = useState<string[]>([]);
  const [mcpArgsText, setMcpArgsText] = useState("{}");
  const [mcpArgsError, setMcpArgsError] = useState<string | null>(null);
  const { logs, setLogs } = useDialogLogs(context.pluginId, open);

  useEffect(() => {
    if (!open) return;
    setInputKind("text");
    setTextInput("");
    setImagePath(null);
    setFilePaths([]);
    if (target.kind === "mcp") {
      setMcpArgsText(
        serializeMcpArguments(buildMcpArgumentsStub(target.item.inputSchema)),
      );
      setMcpArgsError(null);
    } else {
      setMcpArgsText("{}");
      setMcpArgsError(null);
    }
  }, [open, target]);

  const runVerify = async () => {
    if (!context.connected || !context.pluginId) return;
    setBusy(true);
    setLogs([]);
    try {
      switch (target.kind) {
        case "command": {
          const params = buildCommandVerifyParams(
            target.item.id,
            inputKind,
            textInput,
            imagePath,
            filePaths,
          );
          if (!params) {
            toast.error(
              inputKind === "text"
                ? "请输入字符串内容"
                : inputKind === "image"
                  ? "请选择图片"
                  : "请选择文件",
            );
            return;
          }
          await api.pluginCallCommand(
            context.pluginId,
            target.item.id,
            params,
          );
          break;
        }
        case "mcp": {
          const parsed = parseMcpArgumentsJson(mcpArgsText);
          if (!parsed.ok) {
            setMcpArgsError(parsed.error);
            return;
          }
          setMcpArgsError(null);
          await api.runPluginDevMcpTool(
            context.projectId,
            target.item.name,
            parsed.value,
          );
          break;
        }
      }
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const canRunCommandInput =
    target.kind !== "command" ||
    (inputKind === "text" && textInput.trim().length > 0) ||
    (inputKind === "image" && Boolean(imagePath)) ||
    (inputKind === "file" && filePaths.length > 0);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button type="button" size="lg" variant="outline" title="验证">
          <Play data-icon="inline-start" />
          验证
        </Button>
      </DialogTrigger>
      <DialogPanel className="max-h-[min(760px,calc(100vh-2rem))] sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>验证 {targetTitle(target)}</DialogTitle>
        </DialogHeader>
        <DialogContent className="flex min-h-0 flex-1 flex-col gap-4">
          {target.kind === "command" ? (
            <FieldGroup className="gap-3">
              <Field>
                <FieldLabel>输入类型</FieldLabel>
                <Select
                  items={[...COMMAND_INPUT_KIND_ITEMS]}
                  value={inputKind}
                  onValueChange={(value) => {
                    if (
                      value === "text" ||
                      value === "image" ||
                      value === "file"
                    ) {
                      setInputKind(value);
                    }
                  }}
                >
                  <SelectTrigger className="w-full" aria-label="输入类型">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {COMMAND_INPUT_KIND_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {inputKind === "text" ? (
                <Field>
                  <FieldLabel>字符串内容</FieldLabel>
                  <Textarea
                    value={textInput}
                    onChange={(event) => setTextInput(event.target.value)}
                    placeholder="输入传给 Command 的文本"
                    rows={4}
                    aria-label="字符串内容"
                  />
                </Field>
              ) : null}
              {inputKind === "image" ? (
                <Field>
                  <FieldLabel>图片</FieldLabel>
                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      size="lg"
                      variant="outline"
                      onClick={async () => {
                        const path = await openNativeFileDialog({
                          multiple: false,
                          directory: false,
                          title: "选择图片",
                          filters: [
                            {
                              name: "图片",
                              extensions: [
                                "png",
                                "jpg",
                                "jpeg",
                                "gif",
                                "webp",
                                "bmp",
                              ],
                            },
                          ],
                        });
                        if (typeof path === "string") setImagePath(path);
                      }}
                    >
                      <FolderOpen data-icon="inline-start" />
                      选择图片
                    </Button>
                    {imagePath ? (
                      <Button
                        type="button"
                        size="icon-lg"
                        variant="ghost"
                        aria-label="清除图片"
                        onClick={() => setImagePath(null)}
                      >
                        <X />
                      </Button>
                    ) : null}
                  </div>
                  <p
                    className="truncate text-[12px] text-muted-foreground"
                    title={imagePath ?? undefined}
                  >
                    {imagePath ?? "未选择图片"}
                  </p>
                </Field>
              ) : null}
              {inputKind === "file" ? (
                <Field>
                  <FieldLabel>文件</FieldLabel>
                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      size="lg"
                      variant="outline"
                      onClick={async () => {
                        const path = await openNativeFileDialog({
                          multiple: true,
                          directory: false,
                          title: "选择文件",
                        });
                        if (typeof path === "string") setFilePaths([path]);
                        else if (Array.isArray(path)) setFilePaths(path);
                      }}
                    >
                      <FolderOpen data-icon="inline-start" />
                      选择文件
                    </Button>
                    {filePaths.length > 0 ? (
                      <Button
                        type="button"
                        size="icon-lg"
                        variant="ghost"
                        aria-label="清除文件"
                        onClick={() => setFilePaths([])}
                      >
                        <X />
                      </Button>
                    ) : null}
                  </div>
                  <p className="text-[12px] text-muted-foreground">
                    {filePaths.length > 0
                      ? filePaths.join("\n")
                      : "未选择文件"}
                  </p>
                </Field>
              ) : null}
            </FieldGroup>
          ) : null}
          {!context.connected ? (
            <p className="plugin-dev-verify__hint">
              先在「连接」页连接到 Tempo 后再运行验证。
            </p>
          ) : null}
          {target.kind === "mcp" ? (
            <Field data-invalid={Boolean(mcpArgsError)}>
              <FieldLabel>输入参数</FieldLabel>
              <Textarea
                className="plugin-dev-schema-editor plugin-dev-verify__args-editor"
                value={mcpArgsText}
                spellCheck={false}
                aria-invalid={Boolean(mcpArgsError)}
                aria-label="输入参数"
                onChange={(event) => {
                  const raw = event.target.value;
                  setMcpArgsText(raw);
                  const parsed = parseMcpArgumentsJson(raw);
                  setMcpArgsError(parsed.ok ? null : parsed.error);
                }}
              />
              <FieldError>{mcpArgsError}</FieldError>
            </Field>
          ) : null}
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between gap-2">
              <FieldLabel>日志</FieldLabel>
            </div>
            <pre className="plugin-dev-output plugin-dev-output--dialog">
              {formatLogs(logs)}
            </pre>
          </div>
        </DialogContent>
        <DialogFooter>
          <DialogClose asChild>
            <Button type="button" size="lg" variant="outline">
              关闭
            </Button>
          </DialogClose>
          <Button
            type="button"
            size="lg"
            onClick={() => void runVerify()}
            disabled={
              busy ||
              !context.connected ||
              !context.pluginId ||
              !canRunCommandInput ||
              (target.kind === "mcp" && Boolean(mcpArgsError))
            }
          >
            {busy ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <Play data-icon="inline-start" />
            )}
            运行验证
          </Button>
        </DialogFooter>
      </DialogPanel>
    </Dialog>
  );
}
