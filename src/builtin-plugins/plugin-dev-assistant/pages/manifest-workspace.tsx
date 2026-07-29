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
import { Spinner } from "@/components/ui/spinner";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import type { EditablePluginManifest } from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import {
  ActionEditor,
  AppEditor,
  CommandEditor,
  ContributionSection,
  HookEditor,
  McpToolEditor,
  SettingEditor,
} from "@/builtin-plugins/plugin-dev-assistant/pages/editors";
import type { ManifestMode } from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import { ManifestRootEditor } from "@/builtin-plugins/plugin-dev-assistant/pages/manifest-root-editor";
import { PluginDevSection } from "@/builtin-plugins/plugin-dev-assistant/components/PluginDevSection";
import type { ContributionVerifyContext } from "@/builtin-plugins/plugin-dev-assistant/components/ContributionVerifyDialog";
import type { PluginDevProjectDetail } from "@/types";

export function ManifestWorkspace({
  detail,
  manifest,
  raw,
  mode,
  busy,
  verifyContext,
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
  verifyContext: ContributionVerifyContext;
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
            <div className="plugin-dev-form">
              <PluginDevSection title="manifest.json">
                <Textarea
                  className="plugin-dev-json-editor"
                  value={raw}
                  onChange={(event) => onRawChange(event.target.value)}
                  spellCheck={false}
                  aria-label="manifest.json"
                />
              </PluginDevSection>
            </div>
          ) : manifest ? (
            <div className="plugin-dev-form">
        <ManifestRootEditor manifest={manifest} onUpdate={onUpdate} />
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
              key={index}
              app={app}
              index={index}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <ContributionSection
          title="Commands"
          description="对外能力：仅 Action / Hook / MCP；对内 UI 通信请用 tempo.ipc，勿占用此表"
          columns={["Command ID", "标题", "可见性"]}
          columnsClassName="grid-cols-[1fr_1.4fr_0.9fr_auto]"
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
              key={index}
              command={command}
              index={index}
              verifyContext={verifyContext}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
        <ContributionSection
          title="Actions"
          description="将主面板输入路由到 App 或 Command"
          columns={["Action ID", "名称", "类型", "目标"]}
          columnsClassName="grid-cols-[minmax(0,1fr)_10rem_8.5rem_minmax(0,1fr)_auto]"
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
              key={index}
              action={action}
              index={index}
              manifest={manifest}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
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
              key={index}
              hook={hook}
              index={index}
              commands={manifest.contributes.commands}
              verifyContext={verifyContext}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
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
              key={index}
              tool={tool}
              index={index}
              commands={manifest.contributes.commands}
              verifyContext={verifyContext}
              onUpdate={onUpdate}
            />
          ))}
        </ContributionSection>
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
              key={index}
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
