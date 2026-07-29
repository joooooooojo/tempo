import type { ReactNode } from "react";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type {
  EditablePluginAction,
  EditablePluginApp,
  EditablePluginCommand,
  EditablePluginHook,
  EditablePluginManifest,
  EditablePluginMcpTool,
  EditablePluginSetting,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  SETTING_TYPE_ITEMS,
  TARGET_ITEMS,
  WINDOW_MODE_ITEMS,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";

export function ContributionSection({
  title,
  description,
  onAdd,
  children,
}: {
  title: string;
  description: string;
  onAdd: () => void;
  children: ReactNode;
}) {
  return (
    <section className="plugin-dev-contribution">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
        <Button size="sm" variant="outline" onClick={onAdd}>
          <Plus data-icon="inline-start" />
          添加
        </Button>
      </div>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
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
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1.4fr_1fr] gap-3">
        <Field>
          <FieldLabel>App ID</FieldLabel>
          <Input
            value={app.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.apps[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>名称</FieldLabel>
          <Input
            value={app.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.apps[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>窗口</FieldLabel>
          <Select
            items={WINDOW_MODE_ITEMS}
            value={app.windowMode ?? "normal"}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                next.contributes.apps[index].windowMode = value as
                  "normal" | "standalone";
              })
            }
          >
            <SelectTrigger className="w-full">
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
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 App"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.apps.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

export function CommandEditor({
  command,
  index,
  onUpdate,
}: {
  command: EditablePluginCommand;
  index: number;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-2 gap-3">
        <Field>
          <FieldLabel>Command ID</FieldLabel>
          <Input
            value={command.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.commands[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>标题</FieldLabel>
          <Input
            value={command.title}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.commands[index].title = event.target.value;
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Command"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.commands.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
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
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1.2fr_0.8fr_1.2fr] gap-3">
        <Field>
          <FieldLabel>Action ID</FieldLabel>
          <Input
            value={action.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.actions[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>名称</FieldLabel>
          <Input
            value={action.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.actions[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>类型</FieldLabel>
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
            <SelectTrigger className="w-full">
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
        </Field>
        <Field>
          <FieldLabel>目标</FieldLabel>
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
            <SelectTrigger className="w-full">
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
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Action"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.actions.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

export function HookEditor({
  hook,
  index,
  commands,
  onUpdate,
}: {
  hook: EditablePluginHook;
  index: number;
  commands: EditablePluginCommand[];
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-2 gap-3">
        <Field>
          <FieldLabel>事件</FieldLabel>
          <Input
            value={hook.event}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.hooks[index].event = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>Command</FieldLabel>
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
            <SelectTrigger className="w-full">
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
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Hook"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.hooks.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

export function McpToolEditor({
  tool,
  index,
  commands,
  onUpdate,
}: {
  tool: EditablePluginMcpTool;
  index: number;
  commands: EditablePluginCommand[];
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const commandItems = commands.map((command) => ({
    value: command.id,
    label: command.title || command.id,
  }));
  return (
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_1fr_1.5fr] gap-3">
        <Field>
          <FieldLabel>Tool 名称</FieldLabel>
          <Input
            value={tool.name}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.mcpTools[index].name = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>Command</FieldLabel>
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
            <SelectTrigger className="w-full">
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
        </Field>
        <Field>
          <FieldLabel>描述</FieldLabel>
          <Input
            value={tool.description}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.mcpTools[index].description =
                  event.target.value;
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 MCP Tool"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.mcpTools.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
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
    <div className="plugin-dev-contribution-row">
      <div className="grid flex-1 grid-cols-[1fr_0.8fr_1.2fr_1fr] gap-3">
        <Field>
          <FieldLabel>Setting ID</FieldLabel>
          <Input
            value={setting.id}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.settings[index].id = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>控件</FieldLabel>
          <Select
            items={SETTING_TYPE_ITEMS}
            value={setting.type}
            onValueChange={(value) =>
              value &&
              onUpdate((next) => {
                const target = next.contributes.settings[index];
                target.type = value as EditablePluginSetting["type"];
                target.default =
                  value === "switch"
                    ? false
                    : value === "multiselect"
                      ? []
                      : "";
                if (value === "select" || value === "multiselect")
                  target.options = target.options?.length
                    ? target.options
                    : [{ value: "default", label: "Default" }];
                else delete target.options;
              })
            }
          >
            <SelectTrigger className="w-full">
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
        </Field>
        <Field>
          <FieldLabel>标题</FieldLabel>
          <Input
            value={setting.title}
            onChange={(event) =>
              onUpdate((next) => {
                next.contributes.settings[index].title = event.target.value;
              })
            }
          />
        </Field>
        <Field>
          <FieldLabel>默认值</FieldLabel>
          <Input
            value={
              typeof setting.default === "string"
                ? setting.default
                : JSON.stringify(setting.default)
            }
            onChange={(event) =>
              onUpdate((next) => {
                const raw = event.target.value;
                try {
                  next.contributes.settings[index].default = JSON.parse(
                    raw,
                  ) as unknown;
                } catch {
                  next.contributes.settings[index].default = raw;
                }
              })
            }
          />
        </Field>
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label="删除 Setting"
        onClick={() =>
          onUpdate((next) => {
            next.contributes.settings.splice(index, 1);
          })
        }
      >
        <Trash2 />
      </Button>
    </div>
  );
}

