import { useEffect, useState } from "react";
import { Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import type {
  EditablePluginCommand,
  EditablePluginManifest,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  connectionBadge,
  messageOf,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import type { PluginDevProjectDetail } from "@/types";

export function TestWorkspace({
  detail,
  manifest,
  commands,
  commandId,
  params,
  result,
  busy,
  onCommandChange,
  onParamsChange,
  onRun,
}: {
  detail: PluginDevProjectDetail;
  manifest: EditablePluginManifest | null;
  commands: EditablePluginCommand[];
  commandId: string | null;
  params: string;
  result: string | null;
  busy: boolean;
  onCommandChange: (value: string | null) => void;
  onParamsChange: (value: string) => void;
  onRun: () => void;
}) {
  const [mode, setMode] = useState<"command" | "hook" | "mcp">("command");
  const [hookEvent, setHookEvent] = useState<string | null>(
    manifest?.contributes.hooks[0]?.event ?? null,
  );
  const [hookPayload, setHookPayload] = useState("{}");
  const [hookResult, setHookResult] = useState<string | null>(null);
  const [mcpToolName, setMcpToolName] = useState<string | null>(
    manifest?.contributes.mcpTools[0]?.name ?? null,
  );
  const [mcpArguments, setMcpArguments] = useState("{}");
  const [mcpResult, setMcpResult] = useState<string | null>(null);
  const [localBusy, setLocalBusy] = useState(false);
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  const hookItems = (manifest?.contributes.hooks ?? []).map((hook) => ({
    value: hook.event,
    label: `${hook.event} -> ${hook.command}`,
  }));
  const mcpItems = (manifest?.contributes.mcpTools ?? []).map((tool) => ({
    value: tool.name,
    label: tool.name,
  }));

  useEffect(() => {
    if (!hookEvent || !hookItems.some((item) => item.value === hookEvent)) {
      setHookEvent(hookItems[0]?.value ?? null);
    }
    if (!mcpToolName || !mcpItems.some((item) => item.value === mcpToolName)) {
      setMcpToolName(mcpItems[0]?.value ?? null);
    }
  }, [hookEvent, hookItems, mcpItems, mcpToolName]);

  const runHook = async () => {
    if (!hookEvent) return;
    setLocalBusy(true);
    try {
      const payload = JSON.parse(hookPayload) as unknown;
      const next = await api.simulatePluginDevHook(
        detail.project.id,
        hookEvent,
        payload,
      );
      setHookResult(JSON.stringify(next, null, 2));
    } catch (error) {
      setHookResult(JSON.stringify({ error: messageOf(error) }, null, 2));
    } finally {
      setLocalBusy(false);
    }
  };

  const runMcpTool = async () => {
    if (!mcpToolName) return;
    setLocalBusy(true);
    try {
      const argumentsValue = JSON.parse(mcpArguments) as unknown;
      const next = await api.runPluginDevMcpTool(
        detail.project.id,
        mcpToolName,
        argumentsValue,
      );
      setMcpResult(JSON.stringify(next, null, 2));
    } catch (error) {
      setMcpResult(JSON.stringify({ error: messageOf(error) }, null, 2));
    } finally {
      setLocalBusy(false);
    }
  };

  return (
    <div className="plugin-dev-panel">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>Headless 测试</h2>
          <p>所有请求通过现有 Supervisor 和平台校验链路。</p>
        </div>
        {connectionBadge(detail.connection)}
      </div>
      <Tabs
        value={mode}
        onValueChange={(value) => setMode(value as typeof mode)}
      >
        <TabsList>
          <TabsTrigger value="command" disabled={commandItems.length === 0}>
            Command
          </TabsTrigger>
          <TabsTrigger value="hook" disabled={hookItems.length === 0}>
            Hook
          </TabsTrigger>
          <TabsTrigger value="mcp" disabled={mcpItems.length === 0}>
            MCP Tool
          </TabsTrigger>
        </TabsList>
        <TabsContent value="command">
          <JsonTestPanel
            label="Command"
            items={commandItems}
            value={commandId}
            inputId="plugin-dev-command-params"
            inputLabel="参数 JSON"
            input={params}
            result={result}
            actionLabel="运行 Command"
            busy={busy}
            enabled={detail.connection.connected && Boolean(commandId)}
            onValueChange={onCommandChange}
            onInputChange={onParamsChange}
            onRun={onRun}
          />
        </TabsContent>
        <TabsContent value="hook">
          <JsonTestPanel
            label="Hook 事件"
            items={hookItems}
            value={hookEvent}
            inputId="plugin-dev-hook-payload"
            inputLabel="事件 Payload"
            input={hookPayload}
            result={hookResult}
            actionLabel="模拟 Hook"
            busy={localBusy}
            enabled={detail.connection.connected && Boolean(hookEvent)}
            onValueChange={setHookEvent}
            onInputChange={setHookPayload}
            onRun={() => void runHook()}
          />
        </TabsContent>
        <TabsContent value="mcp">
          <JsonTestPanel
            label="MCP Tool"
            items={mcpItems}
            value={mcpToolName}
            inputId="plugin-dev-mcp-arguments"
            inputLabel="Arguments JSON"
            input={mcpArguments}
            result={mcpResult}
            actionLabel="运行 MCP Tool"
            busy={localBusy}
            enabled={detail.connection.connected && Boolean(mcpToolName)}
            onValueChange={setMcpToolName}
            onInputChange={setMcpArguments}
            onRun={() => void runMcpTool()}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function JsonTestPanel({
  label,
  items,
  value,
  inputId,
  inputLabel,
  input,
  result,
  actionLabel,
  busy,
  enabled,
  onValueChange,
  onInputChange,
  onRun,
}: {
  label: string;
  items: Array<{ value: string; label: string }>;
  value: string | null;
  inputId: string;
  inputLabel: string;
  input: string;
  result: string | null;
  actionLabel: string;
  busy: boolean;
  enabled: boolean;
  onValueChange: (value: string | null) => void;
  onInputChange: (value: string) => void;
  onRun: () => void;
}) {
  return (
    <div className="plugin-dev-test-grid">
      <FieldGroup>
        <Field>
          <FieldLabel>{label}</FieldLabel>
          <Select items={items} value={value} onValueChange={onValueChange}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {items.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={inputId}>{inputLabel}</FieldLabel>
          <Textarea
            id={inputId}
            className="font-mono"
            rows={12}
            value={input}
            onChange={(event) => onInputChange(event.target.value)}
          />
        </Field>
        <Button onClick={onRun} disabled={busy || !enabled}>
          {busy ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <Play data-icon="inline-start" />
          )}
          {actionLabel}
        </Button>
      </FieldGroup>
      <section>
        <h2>结果</h2>
        <pre className="plugin-dev-command-result">
          {result ?? "运行后在此显示返回值或错误。"}
        </pre>
      </section>
    </div>
  );
}
