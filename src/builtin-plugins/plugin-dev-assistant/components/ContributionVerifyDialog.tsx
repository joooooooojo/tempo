import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Play } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { api } from "@/lib/api";
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

function targetDescription(target: ContributionVerifyTarget) {
  switch (target.kind) {
    case "command":
      return "验证对外 Command（contributes.commands / Action）。MCP Tool 使用独立的 tempo.mcpTools.register，UI 与 Runtime 私有通信使用 ipcRenderer / ipcMain。这里使用默认参数 {}。";
    case "mcp":
      return "验证 Manifest 声明的 MCP Tool 与 Runtime 中同名 tempo.mcpTools.register handler。这里使用默认参数 {}，并执行输入输出 Schema 校验。";
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
  const { logs, setLogs } = useDialogLogs(context.pluginId, open);

  const runVerify = async () => {
    if (!context.connected || !context.pluginId) return;
    setBusy(true);
    setLogs([]);
    try {
      const defaults = {};
      switch (target.kind) {
        case "command":
          await api.pluginCallCommand(
            context.pluginId,
            target.item.id,
            defaults,
          );
          break;
        case "mcp":
          await api.runPluginDevMcpTool(
            context.projectId,
            target.item.name,
            defaults,
          );
          break;
      }
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const description = targetDescription(target);

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
          {description ? (
            <DialogDescription>{description}</DialogDescription>
          ) : null}
        </DialogHeader>
        <DialogContent className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-6">
          {target.kind === "command" ? (
            <FieldGroup>
              <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
                <Field>
                  <FieldLabel>Command ID</FieldLabel>
                  <Input value={target.item.id} readOnly />
                </Field>
                <Field>
                  <FieldLabel>标题</FieldLabel>
                  <Input value={target.item.title} readOnly />
                </Field>
                <Field>
                  <FieldLabel>可见性</FieldLabel>
                  <Input
                    value={target.item.visibility ?? "private"}
                    readOnly
                  />
                </Field>
              </div>
            </FieldGroup>
          ) : null}
          {!context.connected ? (
            <p className="plugin-dev-verify__hint">
              先在「连接」页连接到 Tempo 后再运行验证。
            </p>
          ) : null}
          <div className="flex min-h-0 flex-col gap-2">
            <div className="flex items-center justify-between gap-2">
              <FieldLabel>日志</FieldLabel>
              <Badge variant="outline">{logs.length} 行</Badge>
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
            disabled={busy || !context.connected || !context.pluginId}
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
