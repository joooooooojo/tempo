import {
  Check,
  Save,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import {
  resolvedManifestKind,
  type EditablePluginManifest,
  type PluginKind,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  ActionEditor,
  AppEditor,
  CommandEditor,
  ContributionSection,
  HookEditor,
  McpToolEditor,
  SettingEditor,
} from "@/builtin-plugins/plugin-dev-assistant/pages/editors";
import {
  KIND_ITEMS,
  type ManifestMode,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import type { PluginDevProjectDetail } from "@/types";

export function ManifestWorkspace({
  detail,
  manifest,
  raw,
  mode,
  busy,
  onRawChange,
  onUpdate,
  onSave,
  onForget,
}: {
  detail: PluginDevProjectDetail;
  manifest: EditablePluginManifest | null;
  raw: string;
  mode: ManifestMode;
  busy: boolean;
  onRawChange: (raw: string) => void;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
  onSave: () => void;
  onForget: () => void;
}) {
  const diagnostics = detail.manifest.diagnostics;
  return (
    <div className="plugin-dev-panel plugin-dev-panel--footed">
      <ScrollArea className="plugin-dev-panel__scroll" aria-label="Manifest">
        <div className="plugin-dev-panel__body">
          {mode === "json" ? (
            <Textarea
              className="plugin-dev-json-editor"
              value={raw}
              onChange={(event) => onRawChange(event.target.value)}
              spellCheck={false}
              aria-label="manifest.json"
            />
          ) : manifest ? (
            <div className="plugin-dev-form">
        <FieldSet>
          <FieldLegend>基础信息</FieldLegend>
          <FieldGroup>
            <div className="grid grid-cols-3 gap-4">
              <Field>
                <FieldLabel htmlFor="manifest-id">插件 ID</FieldLabel>
                <Input
                  id="manifest-id"
                  value={manifest.id ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.id = event.target.value;
                    })
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="manifest-name">名称</FieldLabel>
                <Input
                  id="manifest-name"
                  value={manifest.name ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.name = event.target.value;
                    })
                  }
                />
              </Field>
              <Field>
                <FieldLabel>插件类型</FieldLabel>
                <Select
                    items={KIND_ITEMS}
                    value={resolvedManifestKind(manifest)}
                    onValueChange={(value) =>
                        value &&
                        onUpdate((next) => {
                          const nextKind = value as PluginKind;
                          next.kind = nextKind;
                          if (nextKind === "ui") {
                            delete next.main;
                            if (next.contributes.apps.length === 0) {
                              next.contributes.apps.push({
                                id: "main",
                                name: next.name || "Main",
                                entry: "index.html",
                                keywords: [],
                                windowMode: "normal",
                              });
                            }
                          } else if (nextKind === "headless") {
                            next.main = next.main || "main.mjs";
                            next.contributes.apps = [];
                          } else {
                            next.main = next.main || "main.mjs";
                            if (next.contributes.apps.length === 0) {
                              next.contributes.apps.push({
                                id: "main",
                                name: next.name || "Main",
                                entry: "index.html",
                                keywords: [],
                                windowMode: "normal",
                              });
                            }
                          }
                        })
                    }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {KIND_ITEMS.map((item) => (
                          <SelectItem key={item.value} value={item.value}>
                            {item.label}
                          </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <div className="grid grid-cols-3 gap-4">
              <Field>
                <FieldLabel htmlFor="manifest-version">版本</FieldLabel>
                <Input
                  id="manifest-version"
                  value={manifest.version ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.version = event.target.value;
                    })
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="manifest-tempo">Tempo</FieldLabel>
                <Input
                  id="manifest-tempo"
                  value={manifest.engines.tempo ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.engines.tempo = event.target.value;
                    })
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="manifest-api">Plugin API</FieldLabel>
                <Input
                  id="manifest-api"
                  value={manifest.engines.pluginApi ?? ""}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.engines.pluginApi = event.target.value;
                    })
                  }
                />
              </Field>
            </div>
            <Field>
              <FieldLabel htmlFor="manifest-description">描述</FieldLabel>
              <Textarea
                id="manifest-description"
                rows={3}
                value={manifest.description ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.description = event.target.value;
                  })
                }
              />
            </Field>
            {manifest.main !== undefined ? (
              <Field>
                <FieldLabel htmlFor="manifest-main">
                  Runtime 入口
                </FieldLabel>
                <Input
                  id="manifest-main"
                  value={manifest.main}
                  onChange={(event) =>
                    onUpdate((next) => {
                      next.main = event.target.value;
                    })
                  }
                />
              </Field>
            ) : null}
          </FieldGroup>
        </FieldSet>
        <Separator />
        <ContributionSection
          title="Apps"
          description="平台中可打开的插件页面"
          columns={["App ID", "名称", "窗口"]}
          columnsClassName="grid-cols-[1fr_1.4fr_1fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.apps.push({
                id: `app-${next.contributes.apps.length + 1}`,
                name: "New App",
                entry: "index.html",
                keywords: [],
                windowMode: "normal",
              }),
            )
          }
        >
          {manifest.contributes.apps.map((app, index) => (
            <AppEditor
              key={`${app.id}-${index}`}
              app={app}
              index={index}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <Separator />
        <ContributionSection
          title="Commands"
          description="由 Runtime 注册并通过真实 RPC 调用"
          columns={["Command ID", "标题"]}
          columnsClassName="grid-cols-[1fr_1.4fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.commands.push({
                id: `command-${next.contributes.commands.length + 1}`,
                title: "New Command",
                visibility: "private",
              }),
            )
          }
        >
          {manifest.contributes.commands.map((command, index) => (
            <CommandEditor
              key={`${command.id}-${index}`}
              command={command}
              index={index}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <Separator />
        <ContributionSection
          title="Actions"
          description="将主面板输入路由到 App 或 Command"
          columns={["Action ID", "名称", "类型", "目标"]}
          columnsClassName="grid-cols-[1fr_1.2fr_0.8fr_1.2fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.actions.push({
                id: `action-${next.contributes.actions.length + 1}`,
                name: "New Action",
                accepts: ["text"],
                app: next.contributes.apps[0]?.id,
              }),
            )
          }
        >
          {manifest.contributes.actions.map((action, index) => (
            <ActionEditor
              key={`${action.id}-${index}`}
              action={action}
              index={index}
              manifest={manifest}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <Separator />
        <ContributionSection
          title="Hooks"
          description="将平台事件路由到 Runtime Command"
          columns={["事件", "Command"]}
          columnsClassName="grid-cols-[1fr_1fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.hooks.push({
                event: "clipboard.changed",
                command: next.contributes.commands[0]?.id ?? "",
              }),
            )
          }
        >
          {manifest.contributes.hooks.map((hook, index) => (
            <HookEditor
              key={`${hook.event}-${index}`}
              hook={hook}
              index={index}
              commands={manifest.contributes.commands}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <Separator />
        <ContributionSection
          title="MCP Tools"
          description="在助手内测试，开发态默认不向外暴露"
          columns={["Tool 名称", "Command", "描述"]}
          columnsClassName="grid-cols-[1fr_1fr_1.5fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.mcpTools.push({
                name: `tool-${next.contributes.mcpTools.length + 1}`,
                description: "Tool description",
                command: next.contributes.commands[0]?.id ?? "",
                inputSchema: { type: "object", properties: {} },
              }),
            )
          }
        >
          {manifest.contributes.mcpTools.map((tool, index) => (
            <McpToolEditor
              key={`${tool.name}-${index}`}
              tool={tool}
              index={index}
              commands={manifest.contributes.commands}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <Separator />
        <ContributionSection
          title="Settings"
          description="由 Tempo 使用内置控件渲染的插件设置"
          columns={["Setting ID", "控件", "标题", "默认值"]}
          columnsClassName="grid-cols-[1fr_0.8fr_1.2fr_1fr_auto]"
          onAdd={() =>
            onUpdate((next) =>
              next.contributes.settings.push({
                id: `setting-${next.contributes.settings.length + 1}`,
                type: "switch",
                title: "New Setting",
                default: false,
              }),
            )
          }
        >
          {manifest.contributes.settings.map((setting, index) => (
            <SettingEditor
              key={`${setting.id}-${index}`}
              setting={setting}
              index={index}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
            </div>
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <TriangleAlert />
                </EmptyMedia>
                <EmptyTitle>JSON 尚不可视化</EmptyTitle>
                <EmptyDescription>
                  修复 JSON 语法后即可切换到可视化编辑。
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </div>
      </ScrollArea>

      <footer className="plugin-dev-panel__footer">
        {diagnostics.length > 0 ? (
          <div
            className="plugin-dev-status plugin-dev-status--error"
            role="alert"
            title={diagnostics
              .map((diagnostic) =>
                diagnostic.line
                  ? `${diagnostic.line}:${diagnostic.column ?? 1} ${diagnostic.message}`
                  : diagnostic.message,
              )
              .join("\n")}
          >
            <TriangleAlert aria-hidden="true" />
            <span>
              {diagnostics.length === 1
                ? diagnostics[0].message
                : `${diagnostics.length} 项校验问题`}
            </span>
          </div>
        ) : (
          <div className="plugin-dev-status plugin-dev-status--ok">
            <Check aria-hidden="true" />
            <span>Manifest 校验通过</span>
          </div>
        )}
        <div className="flex gap-2">
          <Button size="lg" variant="ghost" onClick={onForget}>
            <Trash2 data-icon="inline-start" />
            移除记录
          </Button>
          <Button size="lg" onClick={onSave} disabled={busy}>
            {busy ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <Save data-icon="inline-start" />
            )}
            保存 Manifest
          </Button>
        </div>
      </footer>
    </div>
  );
}
