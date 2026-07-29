import { Children, type ReactNode } from "react";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  FieldTitle,
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import {
  SchemaObjectField,
  ManifestDetailsDialog,
  StringListField,
  ToggleListField,
} from "@/builtin-plugins/plugin-dev-assistant/components/ManifestControls";
import {
  ContributionVerifyDialog,
  type ContributionVerifyContext,
} from "@/builtin-plugins/plugin-dev-assistant/components/ContributionVerifyDialog";
import { PluginDevSection } from "@/builtin-plugins/plugin-dev-assistant/components/PluginDevSection";
import { cn } from "@/lib/utils";
import type {
  EditableAppRect,
  EditablePluginAction,
  EditablePluginApp,
  EditablePluginCommand,
  EditablePluginHook,
  EditablePluginManifest,
  EditablePluginMcpTool,
  EditablePluginSetting,
  RectValue,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  SETTING_TYPE_ITEMS,
  TARGET_ITEMS,
  WINDOW_MODE_ITEMS,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";

const COMMAND_VISIBILITY_ITEMS = [
  { value: "private", label: "Private" },
  { value: "public", label: "Public" },
] as const;

const APP_ENTRY_ITEMS = [{ value: "index.html", label: "index.html" }] as const;

const HOOK_EVENT_ITEMS = [
  { value: "clipboard.changed", label: "clipboard.changed" },
] as const;

const ACTION_INPUT_ITEMS = [
  { value: "text", label: "文本" },
  { value: "image", label: "图片" },
  { value: "file", label: "文件" },
] as const;

type ActionInput = (typeof ACTION_INPUT_ITEMS)[number]["value"];
type McpAnnotationKey =
  | "readOnlyHint"
  | "destructiveHint"
  | "idempotentHint"
  | "openWorldHint";

const MCP_ANNOTATION_ITEMS: Array<{
  value: McpAnnotationKey;
  label: string;
}> = [
  { value: "readOnlyHint", label: "只读操作" },
  { value: "destructiveHint", label: "可能产生破坏性修改" },
  { value: "idempotentHint", label: "重复调用结果一致" },
  { value: "openWorldHint", label: "会与外部系统交互" },
];

function setOptionalString(
  target: Record<string, unknown>,
  key: string,
  value: string,
) {
  if (value === "") delete target[key];
  else target[key] = value;
}

function parseRectValue(raw: string): RectValue | undefined {
  const value = raw.trim();
  if (!value) return undefined;
  if (/^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(value)) return Number(value);
  return value;
}

function updateRectValue(
  app: EditablePluginApp,
  key: keyof EditableAppRect,
  raw: string,
) {
  const value = parseRectValue(raw);
  const rect = app.rect ?? {};
  if (value === undefined) delete rect[key];
  else rect[key] = value;
  if (Object.keys(rect).length === 0) delete app.rect;
  else app.rect = rect;
}

function RowActions({ children }: { children: ReactNode }) {
  return <div className="plugin-dev-row-actions">{children}</div>;
}

function DeleteRowButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button
      type="button"
      size="icon-lg"
      variant="ghost"
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <Trash2 />
    </Button>
  );
}

export function ContributionSection({
  title,
  description,
  onAdd,
  columns,
  columnsClassName,
  children,
}: {
  title: string;
  description: string;
  onAdd: () => void;
  columns: string[];
  columnsClassName: string;
  children: ReactNode;
}) {
  const isEmpty = Children.count(children) === 0;

  return (
    <PluginDevSection
      title={title}
      description={description}
      action={
        <Button type="button" size="lg" variant="outline" onClick={onAdd}>
          <Plus data-icon="inline-start" />
          添加
        </Button>
      }
      className="plugin-dev-contribution"
      contentClassName="gap-0 p-0"
    >
      {isEmpty ? (
        <p className="plugin-dev-contribution-empty">暂无数据</p>
      ) : (
        <div className="plugin-dev-contribution-table">
          <div
            className={cn(
              "plugin-dev-contribution-header",
              columnsClassName,
            )}
          >
            {columns.map((column) => (
              <span key={column}>{column}</span>
            ))}
            <span className="sr-only">操作</span>
          </div>
          <div className="plugin-dev-contribution-body">{children}</div>
        </div>
      )}
    </PluginDevSection>
  );
}

export function AppEditor({
  app,
  index,
  onUpdate,
}: {
  app: EditablePluginApp;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_1.4fr_1fr_auto]",
      )}
    >
      <Input
        aria-label="App ID"
        value={app.id}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.apps[index].id = event.target.value;
          })
        }
      />
      <Input
        aria-label="名称"
        value={app.name}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.apps[index].name = event.target.value;
          })
        }
      />
      <Select
        items={WINDOW_MODE_ITEMS}
        value={app.windowMode ?? "normal"}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.apps[index].windowMode = value as
              | "normal"
              | "standalone";
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="窗口">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {WINDOW_MODE_ITEMS.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <RowActions>
        <ManifestDetailsDialog
          title={app.name || app.id || "App 配置"}
          description="配置页面入口、搜索信息、窗口尺寸与会话版本"
        >
          <FieldGroup>
            <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
              <Field>
                <FieldLabel>页面入口</FieldLabel>
                <Select
                  items={APP_ENTRY_ITEMS}
                  value={app.entry || null}
                  onValueChange={(value) =>
                    value &&
                    onUpdate((next) => {
                      next.contributes.apps[index].entry = value;
                    })
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {APP_ENTRY_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel>图标路径</FieldLabel>
                <Input
                  value={app.icon ?? ""}
                  placeholder="icons/app.svg"
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(
                        next.contributes.apps[index],
                        "icon",
                        event.target.value,
                      ),
                    )
                  }
                />
              </Field>
            </div>
            <StringListField
              label="搜索关键词"
              itemLabel="关键词"
              items={app.keywords ?? []}
              onChange={(items) =>
                onUpdate((next) => {
                  next.contributes.apps[index].keywords = items;
                })
              }
            />
            <FieldSet>
              <FieldLegend variant="label">窗口尺寸与位置</FieldLegend>
              <div className="plugin-dev-form-grid plugin-dev-form-grid--4">
                {(
                  [
                    ["width", "宽度", "输入宽度"],
                    ["height", "高度", "输入高度"],
                    ["x", "X", "输入左部距离"],
                    ["y", "Y", "输入顶部距离"],
                  ] as const
                ).map(([key, label, placeholder]) => (
                  <Field key={key}>
                    <FieldLabel>{label}</FieldLabel>
                    <Input
                      value={app.rect?.[key]?.toString() ?? ""}
                      placeholder={placeholder}
                      onChange={(event) =>
                        onUpdate((next) =>
                          updateRectValue(
                            next.contributes.apps[index],
                            key,
                            event.target.value,
                          ),
                        )
                      }
                    />
                  </Field>
                ))}
              </div>
            </FieldSet>
            <Field>
              <FieldLabel>会话版本</FieldLabel>
              <Input
                type="number"
                min={1}
                step={1}
                value={app.sessionVersion ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    const target = next.contributes.apps[index];
                    if (!event.target.value) delete target.sessionVersion;
                    else target.sessionVersion = Number(event.target.value);
                  })
                }
              />
            </Field>
          </FieldGroup>
        </ManifestDetailsDialog>
        <DeleteRowButton
          label="删除 App"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.apps.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}

export function CommandEditor({
  command,
  index,
  verifyContext,
  onUpdate,
}: {
  command: EditablePluginCommand;
  index: number;
  verifyContext?: ContributionVerifyContext;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_1.4fr_0.9fr_auto]",
      )}
    >
      <Input
        aria-label="Command ID"
        value={command.id}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.commands[index].id = event.target.value;
          })
        }
      />
      <Input
        aria-label="标题"
        value={command.title}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.commands[index].title = event.target.value;
          })
        }
      />
      <Select
        items={COMMAND_VISIBILITY_ITEMS}
        value={command.visibility ?? "private"}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.commands[index].visibility = value as
              | "private"
              | "public";
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="可见性">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {COMMAND_VISIBILITY_ITEMS.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <RowActions>
        {verifyContext ? (
          <ContributionVerifyDialog
            target={{ kind: "command", item: command }}
            context={verifyContext}
          />
        ) : null}
        <DeleteRowButton
          label="删除 Command"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.commands.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}

export function ActionEditor({
  action,
  index,
  manifest,
  onUpdate,
}: {
  action: EditablePluginAction;
  index: number;
  manifest: EditablePluginManifest;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const targetKind = action.command ? "command" : "app";
  const targetItems =
    targetKind === "command"
      ? manifest.contributes.commands.map((item) => ({
          value: item.id,
          label: item.title || item.id,
        }))
      : manifest.contributes.apps.map((item) => ({
          value: item.id,
          label: item.name || item.id,
        }));
  const acceptedInputs = action.accepts?.length
    ? action.accepts
    : (["text"] as ActionInput[]);

  return (
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[minmax(0,1fr)_10rem_8.5rem_minmax(0,1fr)_auto]",
      )}
    >
      <Input
        aria-label="Action ID"
        value={action.id}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.actions[index].id = event.target.value;
          })
        }
      />
      <Input
        aria-label="名称"
        value={action.name}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.actions[index].name = event.target.value;
          })
        }
      />
      <Select
        items={TARGET_ITEMS}
        value={targetKind}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            const target = next.contributes.actions[index];
            delete target.app;
            delete target.command;
            if (value === "command")
              target.command = next.contributes.commands[0]?.id ?? "";
            else target.app = next.contributes.apps[0]?.id ?? "";
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="类型">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {TARGET_ITEMS.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Select
        items={targetItems}
        value={action.command ?? action.app ?? null}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            const target = next.contributes.actions[index];
            if (targetKind === "command") target.command = value;
            else target.app = value;
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="目标">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {targetItems.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <RowActions>
        <ManifestDetailsDialog
          title={action.name || action.id || "Action 配置"}
          description="配置推荐条件、搜索信息和动态标题"
        >
          <FieldGroup>
            <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
              <Field>
                <FieldLabel>图标路径</FieldLabel>
                <Input
                  value={action.icon ?? ""}
                  placeholder="icons/action.svg"
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(
                        next.contributes.actions[index],
                        "icon",
                        event.target.value,
                      ),
                    )
                  }
                />
              </Field>
              <Field>
                <FieldLabel>标题模板</FieldLabel>
                <Input
                  value={action.titleTemplate ?? ""}
                  placeholder="处理 {query}"
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(
                        next.contributes.actions[index],
                        "titleTemplate",
                        event.target.value,
                      ),
                    )
                  }
                />
              </Field>
            </div>
            <ToggleListField<ActionInput>
              legend="接受的输入"
              options={ACTION_INPUT_ITEMS}
              values={acceptedInputs}
              requireOne
              onChange={(values) =>
                onUpdate((next) => {
                  const target = next.contributes.actions[index];
                  delete target.requiresQuery;
                  target.accepts = values;
                })
              }
            />
            <StringListField
              label="搜索关键词"
              itemLabel="关键词"
              items={action.keywords ?? []}
              onChange={(items) =>
                onUpdate((next) => {
                  next.contributes.actions[index].keywords = items;
                })
              }
            />
          </FieldGroup>
        </ManifestDetailsDialog>
        <DeleteRowButton
          label="删除 Action"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.actions.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}

export function HookEditor({
  hook,
  index,
  commands,
  verifyContext,
  onUpdate,
}: {
  hook: EditablePluginHook;
  index: number;
  commands: EditablePluginCommand[];
  verifyContext?: ContributionVerifyContext;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div
      className={cn("plugin-dev-contribution-row", "grid-cols-[1fr_1fr_auto]")}
    >
      <Select
        items={HOOK_EVENT_ITEMS}
        value={hook.event || null}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.hooks[index].event = value;
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="事件">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {HOOK_EVENT_ITEMS.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Select
        items={commandItems}
        value={hook.command}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.hooks[index].command = value;
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="Command">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {commandItems.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <RowActions>
        {verifyContext ? (
          <ContributionVerifyDialog
            target={{ kind: "hook", item: hook }}
            context={verifyContext}
          />
        ) : null}
        <DeleteRowButton
          label="删除 Hook"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.hooks.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}

export function McpToolEditor({
  tool,
  index,
  commands,
  verifyContext,
  onUpdate,
}: {
  tool: EditablePluginMcpTool;
  index: number;
  commands: EditablePluginCommand[];
  verifyContext?: ContributionVerifyContext;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_1fr_1.5fr_auto]",
      )}
    >
      <Input
        aria-label="Tool 名称"
        value={tool.name}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.mcpTools[index].name = event.target.value;
          })
        }
      />
      <Select
        items={commandItems}
        value={tool.command}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.mcpTools[index].command = value;
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="Command">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {commandItems.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Input
        aria-label="描述"
        value={tool.description}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.mcpTools[index].description = event.target.value;
          })
        }
      />
      <RowActions>
        {verifyContext ? (
          <ContributionVerifyDialog
            target={{ kind: "mcp", item: tool }}
            context={verifyContext}
          />
        ) : null}
        <ManifestDetailsDialog
          title={tool.name || "MCP Tool 配置"}
          description="配置输入输出契约和 MCP 客户端提示"
          panelClassName="sm:max-w-3xl"
          contentClassName="gap-5"
        >
          <Field>
            <FieldLabel>描述</FieldLabel>
            <Textarea
              rows={3}
              value={tool.description}
              onChange={(event) =>
                onUpdate((next) => {
                  next.contributes.mcpTools[index].description = event.target.value;
                })
              }
            />
          </Field>
          <Tabs defaultValue="input" className="plugin-dev-mcp-tabs">
            <TabsList className="w-full">
              <TabsTrigger value="input">Input Schema</TabsTrigger>
              <TabsTrigger value="output">Output Schema</TabsTrigger>
              <TabsTrigger value="annotations">Annotations</TabsTrigger>
            </TabsList>
            <TabsContent value="input" className="plugin-dev-mcp-tabs__content">
              <SchemaObjectField
                label="参数字段"
                description="为 MCP 客户端声明调用参数"
                value={tool.inputSchema ?? { type: "object", properties: {} }}
                onChange={(value) =>
                  onUpdate((next) => {
                    next.contributes.mcpTools[index].inputSchema = value;
                  })
                }
              />
            </TabsContent>
            <TabsContent value="output" className="plugin-dev-mcp-tabs__content">
              <Field orientation="horizontal">
                <FieldContent>
                  <FieldTitle>声明返回结构</FieldTitle>
                  <FieldDescription>校验 Runtime 返回结果</FieldDescription>
                </FieldContent>
                <Switch
                  checked={Boolean(tool.outputSchema)}
                  onCheckedChange={(checked) =>
                    onUpdate((next) => {
                      const target = next.contributes.mcpTools[index];
                      if (checked)
                        target.outputSchema = { type: "object", properties: {} };
                      else delete target.outputSchema;
                    })
                  }
                />
              </Field>
              {tool.outputSchema ? (
                <SchemaObjectField
                  label="返回字段"
                  description="为 MCP 客户端声明返回结果"
                  value={tool.outputSchema}
                  onChange={(value) =>
                    onUpdate((next) => {
                      next.contributes.mcpTools[index].outputSchema = value;
                    })
                  }
                />
              ) : null}
            </TabsContent>
            <TabsContent
              value="annotations"
              className="plugin-dev-mcp-tabs__content"
            >
              <FieldSet>
                <FieldLegend variant="label">客户端提示</FieldLegend>
                <div className="plugin-dev-toggle-grid">
                  {MCP_ANNOTATION_ITEMS.map((annotation) => (
                    <Field key={annotation.value} orientation="horizontal">
                      <FieldContent>
                        <FieldTitle>{annotation.label}</FieldTitle>
                        <FieldDescription>{annotation.value}</FieldDescription>
                      </FieldContent>
                      <Switch
                        size="sm"
                        checked={Boolean(tool.annotations?.[annotation.value])}
                        onCheckedChange={(checked) =>
                          onUpdate((next) => {
                            const target = next.contributes.mcpTools[index];
                            target.annotations = {
                              ...target.annotations,
                              [annotation.value]: checked,
                            };
                          })
                        }
                      />
                    </Field>
                  ))}
                </div>
              </FieldSet>
            </TabsContent>
          </Tabs>
        </ManifestDetailsDialog>
        <DeleteRowButton
          label="删除 MCP Tool"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.mcpTools.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}

function SettingDefaultEditor({
  setting,
  index,
  onUpdate,
  compact = false,
}: {
  setting: EditablePluginSetting;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
  compact?: boolean;
}) {
  const options = setting.options ?? [];
  if (setting.type === "switch") {
    return (
      <Switch
        checked={Boolean(setting.default)}
        aria-label="默认值"
        onCheckedChange={(checked) =>
          onUpdate((next) => {
            next.contributes.settings[index].default = checked;
          })
        }
      />
    );
  }
  if (setting.type === "select") {
    const items = options.map((option) => ({
      value: option.value,
      label: option.label || option.value,
    }));
    return (
      <Select
        items={items}
        value={typeof setting.default === "string" ? setting.default : null}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            next.contributes.settings[index].default = value;
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="默认值">
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
    );
  }
  if (setting.type === "multiselect") {
    const selected = Array.isArray(setting.default)
      ? setting.default.filter((item): item is string => typeof item === "string")
      : [];
    if (compact) {
      return <Input aria-label="默认值" value={selected.join(", ")} readOnly />;
    }
    return (
      <ToggleListField
        options={options.map((option) => ({
          value: option.value,
          label: option.label || option.value,
        }))}
        values={selected}
        onChange={(values) =>
          onUpdate((next) => {
            next.contributes.settings[index].default = values;
          })
        }
      />
    );
  }
  return (
    <Input
      aria-label="默认值"
      value={typeof setting.default === "string" ? setting.default : ""}
      onChange={(event) =>
        onUpdate((next) => {
          next.contributes.settings[index].default = event.target.value;
        })
      }
    />
  );
}

function SettingOptionsEditor({
  setting,
  index,
  onUpdate,
}: {
  setting: EditablePluginSetting;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const options = setting.options ?? [];
  return (
    <Field>
      <div className="plugin-dev-field-heading">
        <div>
          <FieldLabel>选项</FieldLabel>
          <FieldDescription>label 用于界面显示，value 用于存储</FieldDescription>
        </div>
        <Button
          type="button"
          size="lg"
          variant="outline"
          onClick={() =>
            onUpdate((next) => {
              const target = next.contributes.settings[index];
              const nextValue = `option-${(target.options?.length ?? 0) + 1}`;
              target.options = [
                ...(target.options ?? []),
                { value: nextValue, label: `Option ${(target.options?.length ?? 0) + 1}` },
              ];
              if (target.type === "select" && !target.default)
                target.default = nextValue;
            })
          }
        >
          <Plus data-icon="inline-start" />
          添加
        </Button>
      </div>
      <div className="plugin-dev-option-list">
        {options.map((option, optionIndex) => (
          <div className="plugin-dev-option-list__row" key={optionIndex}>
            <Input
              aria-label={`选项标签 ${optionIndex + 1}`}
              placeholder="显示名称"
              value={option.label ?? ""}
              onChange={(event) =>
                onUpdate((next) => {
                  const target = next.contributes.settings[index].options?.[optionIndex];
                  if (!target) return;
                  if (event.target.value) target.label = event.target.value;
                  else delete target.label;
                })
              }
            />
            <Input
              aria-label={`选项值 ${optionIndex + 1}`}
              placeholder="存储值"
              value={option.value}
              onChange={(event) =>
                onUpdate((next) => {
                  const target = next.contributes.settings[index];
                  const oldValue = target.options?.[optionIndex]?.value ?? "";
                  const newValue = event.target.value;
                  if (!target.options?.[optionIndex]) return;
                  target.options[optionIndex].value = newValue;
                  if (target.type === "select" && target.default === oldValue)
                    target.default = newValue;
                  if (target.type === "multiselect" && Array.isArray(target.default)) {
                    target.default = target.default.map((value) =>
                      value === oldValue ? newValue : value,
                    );
                  }
                })
              }
            />
            <DeleteRowButton
              label="删除选项"
              onClick={() =>
                onUpdate((next) => {
                  const target = next.contributes.settings[index];
                  const removed = target.options?.[optionIndex]?.value;
                  target.options?.splice(optionIndex, 1);
                  if (target.type === "select" && target.default === removed)
                    target.default = target.options?.[0]?.value ?? "";
                  if (target.type === "multiselect" && Array.isArray(target.default))
                    target.default = target.default.filter((value) => value !== removed);
                })
              }
            />
          </div>
        ))}
      </div>
    </Field>
  );
}

export function SettingEditor({
  setting,
  index,
  onUpdate,
}: {
  setting: EditablePluginSetting;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_0.8fr_1.2fr_1fr_auto]",
      )}
    >
      <Input
        aria-label="Setting ID"
        value={setting.id}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.settings[index].id = event.target.value;
          })
        }
      />
      <Select
        items={SETTING_TYPE_ITEMS}
        value={setting.type}
        onValueChange={(value) =>
          value &&
          onUpdate((next) => {
            const target = next.contributes.settings[index];
            target.type = value as EditablePluginSetting["type"];
            if (value === "switch") {
              target.default = false;
              delete target.options;
              delete target.placeholder;
            } else if (value === "input") {
              target.default = "";
              delete target.options;
            } else {
              target.options = target.options?.length
                ? target.options
                : [{ value: "default", label: "Default" }];
              target.default = value === "multiselect" ? [] : target.options[0].value;
              delete target.placeholder;
            }
          })
        }
      >
        <SelectTrigger className="w-full" aria-label="控件">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {SETTING_TYPE_ITEMS.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Input
        aria-label="标题"
        value={setting.title}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.settings[index].title = event.target.value;
          })
        }
      />
      <SettingDefaultEditor
        compact
        setting={setting}
        index={index}
        onUpdate={onUpdate}
      />
      <RowActions>
        <ManifestDetailsDialog
          title={setting.title || setting.id || "Setting 配置"}
          description="配置设置项说明、默认值和可选项"
        >
          <FieldGroup>
            <Field>
              <FieldLabel>描述</FieldLabel>
              <Textarea
                rows={3}
                value={setting.description ?? ""}
                onChange={(event) =>
                  onUpdate((next) =>
                    setOptionalString(
                      next.contributes.settings[index],
                      "description",
                      event.target.value,
                    ),
                  )
                }
              />
            </Field>
            {setting.type === "input" ? (
              <Field>
                <FieldLabel>Placeholder</FieldLabel>
                <Input
                  value={setting.placeholder ?? ""}
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(
                        next.contributes.settings[index],
                        "placeholder",
                        event.target.value,
                      ),
                    )
                  }
                />
              </Field>
            ) : null}
            {setting.type === "select" || setting.type === "multiselect" ? (
              <SettingOptionsEditor
                setting={setting}
                index={index}
                onUpdate={onUpdate}
              />
            ) : null}
            <Field>
              <FieldLabel>默认值</FieldLabel>
              <SettingDefaultEditor
                setting={setting}
                index={index}
                onUpdate={onUpdate}
              />
            </Field>
          </FieldGroup>
        </ManifestDetailsDialog>
        <DeleteRowButton
          label="删除 Setting"
          onClick={() =>
            onUpdate((next) => {
              next.contributes.settings.splice(index, 1);
            })
          }
        />
      </RowActions>
    </div>
  );
}
