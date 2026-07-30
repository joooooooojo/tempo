export type PluginKind = "ui" | "headless" | "hybrid";
export type PluginCapability =
  | "filesystem"
  | "network"
  | "process"
  | "clipboard"
  | "system";
export type RectValue = number | string;

export interface EditableAppRect {
  width?: RectValue;
  height?: RectValue;
  x?: RectValue;
  y?: RectValue;
}

export interface EditablePluginApp {
  id: string;
  name: string;
  entry: string;
  keywords?: string[];
  icon?: string;
  windowMode?: "normal" | "standalone";
  rect?: EditableAppRect;
  sessionVersion?: number;
  [key: string]: unknown;
}

export interface EditablePluginCommand {
  id: string;
  title: string;
  visibility?: "private" | "public";
  [key: string]: unknown;
}

export interface EditablePluginAction {
  id: string;
  name: string;
  keywords?: string[];
  accepts?: Array<"text" | "image" | "file">;
  icon?: string;
  titleTemplate?: string;
  app?: string;
  command?: string;
  [key: string]: unknown;
}

export interface EditablePluginMcpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  outputSchema?: Record<string, unknown>;
  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };
  [key: string]: unknown;
}

export interface EditablePluginSetting {
  id: string;
  type: "switch" | "select" | "multiselect" | "input";
  title: string;
  description?: string;
  default: unknown;
  options?: Array<{ value: string; label?: string }>;
  placeholder?: string;
  [key: string]: unknown;
}

export interface EditablePluginManifest {
  $schema?: string;
  manifestVersion: number;
  id: string;
  name: string;
  version: string;
  description?: string;
  kind?: PluginKind | string;
  author?: string;
  publisher?: string;
  main?: string;
  homepage?: string;
  repository?: string;
  license?: string;
  categories?: string[];
  capabilities?: PluginCapability[];
  activationEvents?: Array<"onStartup">;
  engines: {
    tempo: string;
    pluginApi: string;
    [key: string]: unknown;
  };
  contributes: {
    apps: EditablePluginApp[];
    commands: EditablePluginCommand[];
    actions: EditablePluginAction[];
    settings: EditablePluginSetting[];
    mcpTools: EditablePluginMcpTool[];
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export function parseEditableManifest(
  raw: string,
): EditablePluginManifest | null {
  try {
    const value = JSON.parse(raw) as unknown;
    if (!value || typeof value !== "object" || Array.isArray(value))
      return null;
    const manifest = value as Partial<EditablePluginManifest>;
    if (!manifest.engines || typeof manifest.engines !== "object") return null;
    if (!manifest.contributes || typeof manifest.contributes !== "object")
      return null;
    const contributes =
      manifest.contributes as EditablePluginManifest["contributes"];
    contributes.apps = Array.isArray(contributes.apps) ? contributes.apps : [];
    contributes.commands = Array.isArray(contributes.commands)
      ? contributes.commands
      : [];
    contributes.actions = Array.isArray(contributes.actions)
      ? contributes.actions
      : [];
    contributes.settings = Array.isArray(contributes.settings)
      ? contributes.settings
      : [];
    contributes.mcpTools = Array.isArray(contributes.mcpTools)
      ? contributes.mcpTools
      : [];
    for (const tool of contributes.mcpTools) {
      delete tool.command;
    }
    return manifest as EditablePluginManifest;
  } catch {
    return null;
  }
}

export function stringifyManifest(manifest: EditablePluginManifest): string {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

export function cloneManifest(
  manifest: EditablePluginManifest,
): EditablePluginManifest {
  return JSON.parse(JSON.stringify(manifest)) as EditablePluginManifest;
}

export function resolvedManifestKind(
  manifest: EditablePluginManifest | null,
): PluginKind {
  const hasUi = Boolean(manifest?.contributes.apps.length);
  const hasRuntime = Boolean(manifest?.main);
  if (hasUi && hasRuntime) return "hybrid";
  if (hasUi) return "ui";
  return "headless";
}
