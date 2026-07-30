export interface AppUsage {
  app_name: string;
  process_name: string;
  category: string;
  seconds: number;
  icon_data_url?: string | null;
}

export interface LauncherApp {
  id: string;
  name: string;
  subtitle: string;
  keywords: string[];
  icon_data_url?: string | null;
  pinned: boolean;
  last_used_at?: string | null;
  use_count: number;
}

export interface LauncherUsageItem {
  id: string;
  pinned: boolean;
  last_used_at?: string | null;
  use_count: number;
}

export interface MainPanelSearchContribution {
  id: string;
  name: string;
  keywords: string[];
  source: "builtin" | "plugin";
}

export interface MainPanelSearchMatch {
  source: "launcher" | "contribution";
  id: string;
  score: number;
  app: LauncherApp | null;
}

export interface HourlyData {
  hour: number;
  seconds: number;
}

export interface DailyReport {
  date: string;
  total_seconds: number;
  average_seconds: number;
  peak_hour: number;
  peak_seconds: number;
  hourly: HourlyData[];
  top_apps: AppUsage[];
}

export interface WeeklyDay {
  date: string;
  seconds: number;
  is_over_limit: boolean;
}

export interface WeeklyReport {
  days: WeeklyDay[];
  average_seconds: number;
  daily_limit_seconds: number;
  top_apps: AppUsage[];
}

export interface TodoSubtask {
  id: number;
  todo_id: number;
  title: string;
  completed: boolean;
  sort_order: number;
  created_at: string;
}

export type TodoRecurrence = "none" | "daily" | "weekly" | "monthly";

export interface TodoItem {
  id: number;
  title: string;
  content: string;
  completed: boolean;
  due_at?: string | null;
  pinned_at?: string | null;
  created_at: string;
  completed_at?: string | null;
  recurrence: TodoRecurrence;
  remind_1d: boolean;
  remind_1h: boolean;
  remind_custom_hours?: number | null;
  recurrence_root_id?: number | null;
  next_recurrence_at?: string | null;
  images: TodoImage[];
  notes: TodoNote[];
  subtasks: TodoSubtask[];
  tags: string[];
  image_count?: number;
  lightweight?: boolean;
}

export interface TodoImage {
  id: number;
  todo_id: number;
  data_url: string;
  mime_type: string;
  created_at: string;
}

export interface TodoNote {
  id: number;
  todo_id: number;
  body: string;
  created_at: string;
  images: TodoNoteImage[];
}

export interface TodoNoteImage {
  id: number;
  note_id: number;
  data_url: string;
  mime_type: string;
  created_at: string;
}

export interface Settings {
  autostart: boolean;
  sound_enabled: boolean;
  theme: "light" | "dark" | "system";
  clipboard_monitor_enabled: boolean;
  clipboard_max_entries: number;
  clipboard_paste_mode: "clipboard" | "active_app";
  clipboard_plain_text_only: boolean;
  clipboard_history_retention: "days" | "weeks" | "months" | "years" | "permanent";
  shortcut_main_panel: string;
  shortcut_clipboard_picker: string;
  shortcut_snippet_picker: string;
  storage_dir: string;
  mcp_enabled: boolean;
  mcp_port: number;
  mcp_token: string;
  /** Builtin app ids hidden from the launcher. `settings` cannot be disabled. */
  disabled_builtin_apps: string[];
}

export interface ClipboardEntry {
  id: number;
  content: string;
  kind: "text" | "image" | "file" | string;
  source_app?: string | null;
  source_process?: string | null;
  source_icon_data_url?: string | null;
  image_width?: number | null;
  image_height?: number | null;
  pinned: boolean;
  created_at: string;
}

export interface ClipboardHistoryPage {
  entries: ClipboardEntry[];
  total: number;
  has_more: boolean;
}

export interface MainPanelClipboardSeed {
  kind: "text" | "image" | "file" | string;
  fullText?: string | null;
  entryId?: number | null;
  imageUrl?: string | null;
  imageWidth?: number | null;
  imageHeight?: number | null;
  paths?: string[] | null;
}

export interface Snippet {
  id: number;
  title: string;
  content: string;
  tags: string[];
  group_id?: number | null;
  group_name?: string | null;
  shortcut?: string | null;
  language?: string | null;
  pinned: boolean;
  use_count: number;
  last_used_at?: string | null;
  archived_at?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface SnippetGroup {
  id: number;
  name: string;
  color: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export type ReminderEvent =
  | { type: "todo_due"; todo_id: number; title: string; lead: "1d" | "1h" | "due" | "custom"; hours?: number };

export interface HostsWorkspace {
  path: string;
  writable: boolean;
  authorized: boolean;
  managed: boolean;
  publicContent: string;
  activeProfileId?: string | null;
  profiles: HostsProfile[];
  systemContent: string;
}

export interface HostsProfile {
  id: string;
  name: string;
  updatedAt: string;
  active: boolean;
}

export interface HostsBackup {
  id: string;
  source: string;
  createdAt: string;
  preview: string;
}

export interface PortRecord {
  protocol: "TCP" | "UDP";
  localAddress: string;
  localPort: number;
  remoteAddress?: string | null;
  remotePort?: number | null;
  state: string;
  pid?: number | null;
  processName: string;
  processPath?: string | null;
  processStartedAt?: number | null;
  canTerminate: boolean;
  protectedReason?: string | null;
}

export interface TerminatePortProcessRequest {
  protocol: PortRecord["protocol"];
  localAddress: string;
  localPort: number;
  pid: number;
  processStartedAt: number;
}

export type TranslateProviderId = "youdao" | "baidu" | "tencent" | "google" | "deepl";

export interface TranslateProviderCreds {
  enabled: boolean;
  fields: Record<string, string>;
}

export interface TranslateConfig {
  defaultProvider: string;
  defaultSourceLang: string;
  defaultTargetLang: string;
  compareMode: boolean;
  providers: Record<string, TranslateProviderCreds>;
}

export interface TranslateResult {
  provider: string;
  text: string;
  detectedFrom?: string | null;
  error?: string | null;
}

export interface RuntimeInstallProgress {
  phase: string;
  message: string;
  downloadedBytes: number;
  totalBytes?: number | null;
  percent?: number | null;
}

export interface PluginRuntimeStatus {
  installed: boolean;
  installing: boolean;
  version?: string | null;
  nodePath?: string | null;
  installDir?: string | null;
  lockedMajor: string;
  message: string;
  progress?: RuntimeInstallProgress | null;
}

export interface InstalledPackage {
  pluginId: string;
  version: string;
  packageHash: string;
  installPath: string;
  requiresNodeRuntime: boolean;
}

export interface InstalledPlugin {
  id: string;
  name: string;
  iconUrl?: string | null;
  currentVersion: string;
  pendingVersion?: string | null;
  enabled: boolean;
  runtimeState: string;
  packageHash?: string | null;
  trusted: boolean;
  installSource: string;
  signatureStatus: string;
  displayPublisher?: string | null;
  requiresNodeRuntime: boolean;
  /** Behavior-derived: `ui` | `hybrid` | `headless`. */
  kind: "ui" | "hybrid" | "headless" | string;
  lastError?: string | null;
  /** User opt-in for exposing this plugin's `contributes.mcpTools` to MCP/AI (design §11). */
  mcpExposed: boolean;
  /** Number of `contributes.mcpTools` this plugin declares (0 = nothing to expose). */
  mcpToolCount: number;
  /** Number of declared MCP tools currently enabled at the per-tool level. */
  mcpEnabledToolCount: number;
  /** Number of `contributes.settings` entries from the current package. */
  settingsCount: number;
  /** First install time for this plugin id (ISO / RFC3339). */
  installedAt: string;
}

/** Host-rendered control kinds for `contributes.settings` (v1). */
export type PluginSettingFieldType =
  | "switch"
  | "select"
  | "multiselect"
  | "input";

export interface PluginSettingOption {
  value: string;
  label?: string | null;
}

export interface PluginSettingField {
  id: string;
  type: PluginSettingFieldType;
  title: string;
  description?: string | null;
  default: unknown;
  options?: PluginSettingOption[] | null;
  placeholder?: string | null;
}

export interface PluginSettingsBundle {
  pluginId: string;
  pluginName: string;
  settings: PluginSettingField[];
  values: Record<string, unknown>;
  mcpToolCount: number;
  mcpExposed: boolean;
}

export interface PluginMcpToolInfo {
  name: string;
  description: string;
  inputSchema?: unknown;
  outputSchema?: unknown;
  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  } | null;
  /** Per-tool preference under the plugin-level MCP master switch. */
  enabled: boolean;
}

export interface BuiltinMcpStatus {
  exposed: boolean;
  toolCount: number;
  enabledToolCount: number;
}

/** Shared shape for MCP method rows in settings dialogs. */
export type McpToolInfo = Pick<PluginMcpToolInfo, "name" | "description" | "enabled">;

export type PluginWindowMode = "normal" | "standalone";
export type PluginRectValue = number | string;

export interface PluginAppRect {
  width?: PluginRectValue | null;
  height?: PluginRectValue | null;
  x?: PluginRectValue | null;
  y?: PluginRectValue | null;
}

export interface PluginAppContribution {
  /** Runtime id: `{pluginId}/{localId}`. */
  id: string;
  localId: string;
  name: string;
  keywords: string[];
  iconUrl?: string | null;
  /** Resolved `tempo-plugin://` URL for the app's UI entry document. */
  entryPath: string;
  windowMode: PluginWindowMode;
  rect: PluginAppRect;
}

export interface PluginActionContribution {
  id: string;
  localId: string;
  name: string;
  keywords: string[];
  iconUrl?: string | null;
  /** Runtime id of the app this action opens: `{pluginId}/{appLocalId}`. */
  appId?: string | null;
  /** Runtime id of the command this action invokes: `{pluginId}/{commandLocalId}`. */
  commandId?: string | null;
  accepts: Array<"text" | "image" | "file">;
  titleTemplate?: string | null;
}

export interface PluginContributionBundle {
  pluginId: string;
  version: string;
  packageHash: string;
  development: boolean;
  developmentUiSource?: "url" | "static" | null;
  name: string;
  description?: string | null;
  requiresNodeRuntime: boolean;
  apps: PluginAppContribution[];
  actions: PluginActionContribution[];
}

export interface PluginUiPrepareResult {
  viewInstanceId: string;
  entryUrl: string;
  theme: string;
  apiVersion: string;
  params: unknown;
  session?: unknown;
}

export interface PluginWindowContext {
  pluginId: string;
  appId: string;
  params: unknown;
}

export interface PluginRpcError {
  code: string;
  message: string;
  data?: unknown;
}

export interface PluginDevProject {
  id: string;
  rootPath: string;
  pluginId?: string | null;
  name?: string | null;
  kind?: string | null;
  lastOpenedAt: string;
  createdAt: string;
  connected: boolean;
}

export interface PluginDevManifestDiagnostic {
  severity: "error" | "warning" | "info" | string;
  code: string;
  line?: number | null;
  column?: number | null;
  message: string;
}

export interface PluginDevManifestDocument {
  raw: string;
  hash: string;
  parsed?: unknown;
  valid: boolean;
  diagnostics: PluginDevManifestDiagnostic[];
}

export interface PluginDevPreferences {
  uiSourceKind?: "url" | "static" | null;
  uiServiceUrl?: string | null;
  uiStaticRoot?: string | null;
  runtimeDevEntry?: string | null;
  autoReconnectRuntime: boolean;
  useProductionData: boolean;
}

export interface PluginDevConnectionStatus {
  connected: boolean;
  pluginId?: string | null;
  state: "disconnected" | "connected" | "partial" | "failed" | string;
  uiState?: string | null;
  runtimeState?: string | null;
  message?: string | null;
}

export interface PluginDevProjectDetail {
  project: PluginDevProject;
  manifest: PluginDevManifestDocument;
  preferences: PluginDevPreferences;
  connection: PluginDevConnectionStatus;
}

export interface PluginDevProbeResult {
  reachable: boolean;
  status?: number | null;
  message: string;
}

export interface PluginDevLogEvent {
  pluginId: string;
  source: "stdout" | "stderr" | string;
  message: string;
  at: string;
}
