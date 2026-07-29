import type { ComponentType, SVGProps } from "react";
import {
  AppWindow,
  Cable,
  Folder,
  FolderOpen,
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
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { PluginDevSection } from "@/builtin-plugins/plugin-dev-assistant/components/PluginDevSection";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { cn } from "@/lib/utils";
import type { PluginKind } from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import { UI_SOURCE_ITEMS } from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
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
  busy: boolean;
  onPreferencesChange: (next: PluginDevPreferences) => void;
  onChooseDirectory: (title: string) => Promise<string | null>;
  onSave: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onReconnectRuntime: () => void;
  onProbe: () => Promise<void>;
  onOpenApp: () => void;
};

export function RuntimeWorkspace({
  detail,
  kind,
  preferences,
  logs,
  busy,
  onPreferencesChange,
  onChooseDirectory,
  onSave,
  onConnect,
  onDisconnect,
  onReconnectRuntime,
  onProbe,
  onOpenApp,
}: RuntimeWorkspaceProps) {
  const hasUi = kind !== "headless";
  const hasRuntime = kind !== "ui";
  const connected = detail.connection.connected;

  return (
    <div className="plugin-dev-panel plugin-dev-panel--footed">
      <ScrollArea className="plugin-dev-panel__scroll" aria-label="连接">
        <div className="plugin-dev-panel__body">
          <div className="plugin-dev-form">
          <PluginDevSection title="连接概览">
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
          </PluginDevSection>
        {hasUi ? (
          <PluginDevSection
            title="UI 连接"
            description="连接外部服务地址或已有静态文件。"
          >
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
          </PluginDevSection>
        ) : null}
        {hasRuntime ? (
          <PluginDevSection
            title="Runtime 连接"
            description="选择外部工具已经生成的 .mjs 或 .js 入口。"
          >
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
          </PluginDevSection>
        ) : null}
        <PluginDevSection
          title="开发数据"
          description="默认隔离 Tempo 管理的 KV 和推荐 dataPath；Runtime 自行读写文件不受此设置限制。"
        >
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
        </PluginDevSection>
        {!hasUi && hasRuntime ? <RuntimeLogsSection logs={logs} /> : null}
          </div>
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

function RuntimeLogsSection({ logs }: { logs: PluginDevLogEvent[] }) {
  const outputText =
    logs.length > 0
      ? logs.map((entry) => `[${entry.source}] ${entry.message}`).join("\n")
      : "暂无日志";

  return (
    <PluginDevSection
      title="Runtime 日志"
      description="stdout / stderr 输出。"
      action={<Badge variant="outline">{logs.length} 行</Badge>}
    >
      <pre className="plugin-dev-output">{outputText}</pre>
    </PluginDevSection>
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
