import type { ComponentType, SVGProps } from "react";
import { useEffect, useState } from "react";
import {
  AppWindow,
  Cable,
  Folder,
  FolderOpen,
  Play,
  RefreshCw,
  Save,
  ServerCog,
  Square,
  TerminalSquare,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import type {
  EditablePluginCommand,
  EditablePluginManifest,
  PluginKind,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  UI_SOURCE_ITEMS,
  messageOf,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import type {
  PluginDevLogEvent,
  PluginDevPreferences,
  PluginDevProjectDetail,
} from "@/types";

type RuntimeWorkspaceProps = {
  detail: PluginDevProjectDetail;
  kind: PluginKind;
  preferences: PluginDevPreferences;
  logs: PluginDevLogEvent[];
  manifest: EditablePluginManifest | null;
  commands: EditablePluginCommand[];
  commandId: string | null;
  params: string;
  result: string | null;
  busy: boolean;
  onPreferencesChange: (next: PluginDevPreferences) => void;
  onChooseDirectory: (title: string) => Promise<string | null>;
  onSave: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onReconnectRuntime: () => void;
  onProbe: () => Promise<void>;
  onOpenApp: () => void;
  onCommandChange: (value: string | null) => void;
  onParamsChange: (value: string) => void;
  onRunCommand: () => void;
};

export function RuntimeWorkspace({
  detail,
  kind,
  preferences,
  logs,
  manifest,
  commands,
  commandId,
  params,
  result,
  busy,
  onPreferencesChange,
  onChooseDirectory,
  onSave,
  onConnect,
  onDisconnect,
  onReconnectRuntime,
  onProbe,
  onOpenApp,
  onCommandChange,
  onParamsChange,
  onRunCommand,
}: RuntimeWorkspaceProps) {
  const hasUi = kind !== "headless";
  const hasRuntime = kind !== "ui";
  const connected = detail.connection.connected;

  return (
    <div className="plugin-dev-panel plugin-dev-panel--footed">
      <ScrollArea className="plugin-dev-panel__scroll" aria-label="连接">
        <div className="plugin-dev-panel__body">
          <div className="plugin-dev-connection-track">
            <ConnectionNode
              icon={Folder}
              title="项目根"
              value="manifest.json"
              active
            />
            <span className="plugin-dev-connection-track__line" />
            <ConnectionNode
              icon={hasUi ? ServerCog : TerminalSquare}
              title={hasUi ? "UI" : "Runtime"}
              value={
                hasUi
                  ? preferences.uiSourceKind === "static"
                    ? "静态目录"
                    : "服务 URL"
                  : "JavaScript"
              }
              active={Boolean(connected)}
            />
            <span className="plugin-dev-connection-track__line" />
            <ConnectionNode
              icon={Cable}
              title="Tempo"
              value={connected ? "已连接" : "未连接"}
              active={connected}
            />
          </div>
          <Separator />
          <div className="plugin-dev-form">
        {hasUi ? (
          <FieldSet>
            <FieldLegend>UI 连接</FieldLegend>
            <FieldDescription>
              连接外部服务地址或已有静态文件。
            </FieldDescription>
            <FieldGroup>
              <Field>
                <FieldLabel>来源</FieldLabel>
                <Select
                  items={UI_SOURCE_ITEMS}
                  value={preferences.uiSourceKind ?? "url"}
                  onValueChange={(value) =>
                    value &&
                    onPreferencesChange({
                      ...preferences,
                      uiSourceKind: value as "url" | "static",
                    })
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {UI_SOURCE_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {preferences.uiSourceKind === "static" ? (
                <Field>
                  <FieldLabel htmlFor="plugin-dev-static-root">
                    静态根目录
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="plugin-dev-static-root"
                      value={preferences.uiStaticRoot ?? ""}
                      placeholder={detail.project.rootPath}
                      onChange={(event) =>
                        onPreferencesChange({
                          ...preferences,
                          uiStaticRoot: event.target.value,
                        })
                      }
                    />
                    <Button
                      size="icon-lg"
                      variant="outline"
                      aria-label="选择静态目录"
                      onClick={async () => {
                        const path =
                          await onChooseDirectory("选择 UI 静态根目录");
                        if (path)
                          onPreferencesChange({
                            ...preferences,
                            uiStaticRoot: path,
                          });
                      }}
                    >
                      <FolderOpen />
                    </Button>
                  </div>
                </Field>
              ) : (
                <Field>
                  <FieldLabel htmlFor="plugin-dev-service-url">
                    服务 URL
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="plugin-dev-service-url"
                      value={preferences.uiServiceUrl ?? ""}
                      placeholder="http://127.0.0.1:5173/"
                      onChange={(event) =>
                        onPreferencesChange({
                          ...preferences,
                          uiServiceUrl: event.target.value,
                        })
                      }
                    />
                    <Button
                      size="lg"
                      variant="outline"
                      onClick={() => void onProbe()}
                    >
                      <RefreshCw data-icon="inline-start" />
                      检测
                    </Button>
                  </div>
                  <FieldDescription>
                    仅接受 localhost、127.0.0.1 或 [::1]。
                  </FieldDescription>
                </Field>
              )}
            </FieldGroup>
          </FieldSet>
        ) : null}
        {hasUi && hasRuntime ? <Separator /> : null}
        {hasRuntime ? (
          <FieldSet>
            <FieldLegend>Runtime 连接</FieldLegend>
            <FieldDescription>
              选择外部工具已经生成的 .mjs 或 .js 入口。
            </FieldDescription>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="plugin-dev-runtime-entry">
                  JavaScript 入口
                </FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="plugin-dev-runtime-entry"
                    value={preferences.runtimeDevEntry ?? ""}
                    placeholder={`${detail.project.rootPath}\\main.mjs`}
                    onChange={(event) =>
                      onPreferencesChange({
                        ...preferences,
                        runtimeDevEntry: event.target.value,
                      })
                    }
                  />
                  <Button
                    size="icon-lg"
                    variant="outline"
                    aria-label="选择 Runtime 入口"
                    onClick={async () => {
                      const path = await openNativeFileDialog({
                        multiple: false,
                        directory: false,
                        title: "选择 Runtime JavaScript 入口",
                        filters: [
                          { name: "JavaScript", extensions: ["js", "mjs"] },
                        ],
                      });
                      if (typeof path === "string")
                        onPreferencesChange({
                          ...preferences,
                          runtimeDevEntry: path,
                        });
                    }}
                  >
                    <FolderOpen />
                  </Button>
                </div>
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="plugin-dev-auto-reconnect">
                  入口变化后自动重连
                </FieldLabel>
                <Switch
                  id="plugin-dev-auto-reconnect"
                  checked={preferences.autoReconnectRuntime}
                  onCheckedChange={(checked) =>
                    onPreferencesChange({
                      ...preferences,
                      autoReconnectRuntime: checked,
                    })
                  }
                />
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="plugin-dev-real-hooks">
                  接收真实 Hook 事件
                </FieldLabel>
                <Switch
                  id="plugin-dev-real-hooks"
                  checked={preferences.receiveRealHooks}
                  onCheckedChange={(checked) =>
                    onPreferencesChange({
                      ...preferences,
                      receiveRealHooks: checked,
                    })
                  }
                />
              </Field>
            </FieldGroup>
          </FieldSet>
        ) : null}
        <Separator />
        <FieldSet>
          <FieldLegend>开发数据</FieldLegend>
          <FieldDescription>
            默认隔离 Tempo 管理的 KV 和推荐 dataPath；Runtime
            自行读写文件不受此设置限制。
          </FieldDescription>
          <FieldGroup>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="plugin-dev-production-data">
                使用正式插件数据
              </FieldLabel>
              <Switch
                id="plugin-dev-production-data"
                checked={preferences.useProductionData}
                onCheckedChange={(checked) =>
                  onPreferencesChange({
                    ...preferences,
                    useProductionData: checked,
                  })
                }
              />
            </Field>
            {preferences.useProductionData ? (
              <FieldDescription>
                开发代码将读取并修改同 ID 正式插件的宿主管理数据。
              </FieldDescription>
            ) : null}
          </FieldGroup>
        </FieldSet>
        {hasRuntime ? (
          <section className="plugin-dev-log">
            <div className="plugin-dev-section-heading">
              <div>
                <h2>Runtime 日志</h2>
                <p>只显示当前开发 Runtime 的 stdout/stderr。</p>
              </div>
              <Badge variant="outline">{logs.length} 行</Badge>
            </div>
            <pre>
              {logs.length
                ? logs
                    .map((entry) => `[${entry.source}] ${entry.message}`)
                    .join("\n")
                : "等待 Runtime 输出..."}
            </pre>
          </section>
        ) : null}
      </div>

          <Separator />

          <VerifySection
            detail={detail}
            manifest={manifest}
            commands={commands}
            commandId={commandId}
            params={params}
            result={result}
            busy={busy}
            connected={connected}
            onCommandChange={onCommandChange}
            onParamsChange={onParamsChange}
            onRunCommand={onRunCommand}
          />
        </div>
      </ScrollArea>

      <footer className="plugin-dev-panel__footer">
        <div className="plugin-dev-status plugin-dev-status--ok">
          <Cable aria-hidden="true" />
          <span>{connected ? "已连接到 Tempo" : "未连接到 Tempo"}</span>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button size="lg" variant="outline" onClick={onSave} disabled={busy}>
            <Save data-icon="inline-start" />
            保存设置
          </Button>
          {connected && hasRuntime ? (
            <Button
              size="lg"
              variant="outline"
              onClick={onReconnectRuntime}
              disabled={busy}
            >
              <RefreshCw data-icon="inline-start" />
              重连 Runtime
            </Button>
          ) : null}
          {connected && hasUi ? (
            <Button size="lg" variant="outline" onClick={onOpenApp}>
              <AppWindow data-icon="inline-start" />
              打开插件
            </Button>
          ) : null}
          {connected ? (
            <Button
              size="lg"
              variant="destructive"
              onClick={onDisconnect}
              disabled={busy}
            >
              <Square data-icon="inline-start" />
              断开
            </Button>
          ) : (
            <Button size="lg" onClick={onConnect} disabled={busy}>
              <Cable data-icon="inline-start" />
              连接到 Tempo
            </Button>
          )}
        </div>
      </footer>
    </div>
  );
}

function VerifySection({
  detail,
  manifest,
  commands,
  commandId,
  params,
  result,
  busy,
  connected,
  onCommandChange,
  onParamsChange,
  onRunCommand,
}: {
  detail: PluginDevProjectDetail;
  manifest: EditablePluginManifest | null;
  commands: EditablePluginCommand[];
  commandId: string | null;
  params: string;
  result: string | null;
  busy: boolean;
  connected: boolean;
  onCommandChange: (value: string | null) => void;
  onParamsChange: (value: string) => void;
  onRunCommand: () => void;
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
    <section className="plugin-dev-verify">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>验证</h2>
          <p>通过现有 Supervisor 调用 Command、模拟 Hook 或运行 MCP Tool。</p>
        </div>
      </div>
      {!connected ? (
        <p className="plugin-dev-verify__hint">先连接到 Tempo 后再运行验证。</p>
      ) : null}
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
            enabled={connected && Boolean(commandId)}
            onValueChange={onCommandChange}
            onInputChange={onParamsChange}
            onRun={onRunCommand}
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
            enabled={connected && Boolean(hookEvent)}
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
            enabled={connected && Boolean(mcpToolName)}
            onValueChange={setMcpToolName}
            onInputChange={setMcpArguments}
            onRun={() => void runMcpTool()}
          />
        </TabsContent>
      </Tabs>
    </section>
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
        <Button onClick={onRun} disabled={busy || !enabled} size="lg">
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

function ConnectionNode({
  icon: Icon,
  title,
  value,
  active,
}: {
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  title: string;
  value: string;
  active: boolean;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-connection-node",
        active && "plugin-dev-connection-node--active",
      )}
    >
      <Icon aria-hidden="true" />
      <span>
        <strong>{title}</strong>
        <small>{value}</small>
      </span>
    </div>
  );
}
