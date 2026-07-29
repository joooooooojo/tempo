import { Children, type ReactNode } from "react";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
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
    <section className="plugin-dev-contribution">
      <div className="plugin-dev-section-heading">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
        <Button size="lg" variant="outline" onClick={onAdd}>
          <Plus data-icon="inline-start" />
          添加
        </Button>
      </div>
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
      <Button
        size="icon-lg"
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
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_1.4fr_auto]",
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
      <Button
        size="icon-lg"
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
    <div
      className={cn(
        "plugin-dev-contribution-row",
        "grid-cols-[1fr_1.2fr_0.8fr_1.2fr_auto]",
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
      <Button
        size="icon-lg"
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
    <div
      className={cn("plugin-dev-contribution-row", "grid-cols-[1fr_1fr_auto]")}
    >
      <Input
        aria-label="事件"
        value={hook.event}
        onChange={(event) =>
          onUpdate((next) => {
            next.contributes.hooks[index].event = event.target.value;
          })
        }
      />
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
      <Button
        size="icon-lg"
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
      <Button
        size="icon-lg"
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
      <Input
        aria-label="默认值"
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
      <Button
        size="icon-lg"
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
