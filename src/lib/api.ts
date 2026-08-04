import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AppUsage,
  ClipboardEntry,
  ClipboardHistoryPage,
  MainPanelClipboardSeed,
  DailyReport,
  HostsBackup,
  HostsProfile,
  HostsWorkspace,
  InstalledPackage,
  InstalledPlugin,
  LauncherApp,
  MainPanelSearchContribution,
  MainPanelSearchMatch,
  LauncherUsageItem,
  CustomLauncherEntry,
  PluginContributionBundle,
  PluginAppRect,
  PluginMcpToolInfo,
  PluginSettingsBundle,
  BuiltinMcpStatus,
  PluginRuntimeStatus,
  PluginDevConnectionStatus,
  PluginDevPreferences,
  PluginDevProbeResult,
  PluginDevProject,
  PluginDevProjectDetail,
  PluginUiPrepareResult,
  PluginWindowContext,
  PortRecord,
  FileSearchPreviewMeta,
  FileSearchArchiveListing,
  FileSearchQueryResult,
  FileSearchStatus,
  Settings,
  ShortcutBindingStatus,
  Snippet,
  SnippetGroup,
  TodoImage,
  TodoItem,
  TodoNote,
  TodoRecurrence,
  TerminatePortProcessRequest,
  TranslateConfig,
  TranslateResult,
  TranslateStreamEvent,
  WeeklyReport,
} from "@/types";

export interface TodoImageInput {
  data_url: string;
  mime_type: string;
}

export const api = {
  getDailyReport: (date?: string) =>
    invoke<DailyReport>("get_daily_report", { date }),
  getWeeklyReport: (endDate?: string) =>
    invoke<WeeklyReport>("get_weekly_report", { endDate }),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Partial<Settings>) =>
    invoke<void>("update_settings", { settings }),
  getShortcutStatuses: () => invoke<ShortcutBindingStatus[]>("get_shortcut_statuses"),
  regenerateMcpToken: () => invoke<Settings>("regenerate_mcp_token"),
  setStorageDir: (storageDir: string) =>
    invoke<Settings>("set_storage_dir", { storageDir }),
  resetToday: () => invoke<void>("reset_today"),
  resetAll: () => invoke<void>("reset_all"),
  getTodos: () => invoke<TodoItem[]>("get_todos"),
  getTodo: (id: number) => invoke<TodoItem>("get_todo", { id }),
  addTodo: (
    title: string,
    content: string,
    dueAt?: string | null,
    images: TodoImageInput[] = [],
    recurrence: TodoRecurrence = "none",
    remind1d = false,
    remind1h = false,
    remindCustomHours: number | null = null,
    subtasks: string[] = [],
    tags: string[] = []
  ) =>
    invoke<TodoItem>("add_todo", {
      title,
      content,
      dueAt,
      images,
      recurrence,
      remind1d,
      remind1h,
      remindCustomHours,
      subtasks,
      tags,
    }),
  updateTodoDetails: (
    id: number,
    title: string,
    content: string,
    dueAt?: string | null,
    recurrence: TodoRecurrence = "none",
    remind1d = false,
    remind1h = false,
    remindCustomHours: number | null = null,
    tags: string[] = []
  ) =>
    invoke<TodoItem>("update_todo_details", {
      id,
      title,
      content,
      dueAt,
      recurrence,
      remind1d,
      remind1h,
      remindCustomHours,
      tags,
    }),
  setTodoCompleted: (id: number, completed: boolean) =>
    invoke<TodoItem>("set_todo_completed", { id, completed }),
  setTodoPinned: (id: number, pinned: boolean) =>
    invoke<TodoItem>("set_todo_pinned", { id, pinned }),
  addTodoSubtask: (todoId: number, title: string) =>
    invoke<TodoItem>("add_todo_subtask", { todoId, title }),
  setTodoSubtaskCompleted: (subtaskId: number, completed: boolean) =>
    invoke<TodoItem>("set_todo_subtask_completed", { subtaskId, completed }),
  updateTodoSubtask: (subtaskId: number, title: string) =>
    invoke<TodoItem>("update_todo_subtask", { subtaskId, title }),
  deleteTodoSubtask: (subtaskId: number) =>
    invoke<TodoItem>("delete_todo_subtask", { subtaskId }),
  deleteTodoImage: (imageId: TodoImage["id"]) =>
    invoke<TodoItem>("delete_todo_image", { imageId }),
  addTodoNote: (todoId: number, body: string, images: TodoImageInput[] = []) =>
    invoke<TodoItem>("add_todo_note", { todoId, body, images }),
  deleteTodoNote: (noteId: number) =>
    invoke<TodoItem>("delete_todo_note", { noteId }),
  restoreTodoNote: (note: TodoNote) =>
    invoke<TodoItem>("restore_todo_note", { note }),
  deleteTodo: (id: number) => invoke<void>("delete_todo", { id }),
  restoreTodo: (todo: TodoItem) => invoke<TodoItem>("restore_todo", { todo }),
  getKnownApps: () => invoke<AppUsage[]>("get_known_apps"),
  getLauncherApps: () => invoke<LauncherApp[]>("get_launcher_apps"),
  refreshLauncherApps: () => invoke<LauncherApp[]>("refresh_launcher_apps"),
  listCustomLauncherEntries: () =>
    invoke<CustomLauncherEntry[]>("list_custom_launcher_entries"),
  addCustomLauncherEntries: (paths: string[]) =>
    invoke<CustomLauncherEntry[]>("add_custom_launcher_entries", { paths }),
  removeCustomLauncherEntry: (id: string) =>
    invoke<void>("remove_custom_launcher_entry", { id }),
  renameCustomLauncherEntry: (id: string, name: string) =>
    invoke<CustomLauncherEntry>("rename_custom_launcher_entry", { id, name }),
  syncMainPanelSearchContributions: (contributions: MainPanelSearchContribution[]) =>
    invoke<void>("sync_main_panel_search_contributions", { contributions }),
  searchMainPanelApps: (query: string, limit?: number) =>
    invoke<MainPanelSearchMatch[]>("search_main_panel_apps", { query, limit }),
  launchIndexedApp: (id: string) => invoke<void>("launch_indexed_app", { id }),
  revealIndexedApp: (id: string) => invoke<void>("reveal_indexed_app", { id }),
  listInstalledUrlBrowsers: () =>
    invoke<
      Array<{
        id: string;
        name: string;
        actionName: string;
        iconDataUrl: string | null;
      }>
    >("list_installed_url_browsers"),
  getDefaultUrlBrowser: () =>
    invoke<{ name: string; iconDataUrl: string | null } | null>(
      "get_default_url_browser",
    ),
  openUrlInBrowser: (url: string, browserId?: string | null) =>
    invoke<void>("open_url_in_browser", {
      url,
      browserId: browserId ?? null,
    }),
  setLauncherAppPinned: (id: string, pinned: boolean) =>
    invoke<void>("set_launcher_app_pinned", { id, pinned }),
  removeLauncherFromRecent: (id: string) =>
    invoke<void>("remove_launcher_from_recent", { id }),
  getLauncherUsage: () => invoke<LauncherUsageItem[]>("get_launcher_usage"),
  recordLauncherUsage: (id: string) => invoke<void>("record_launcher_usage", { id }),
  setMainPanelHeight: (height: number) =>
    invoke<void>("set_main_panel_height", { height }),
  setMainPanelSize: (width: number | null, height: number) =>
    invoke<void>("set_main_panel_size", { width, height }),
  setMainPanelRect: (rect: PluginAppRect) =>
    invoke<void>("set_main_panel_rect", { rect }),
  getMainPanelPosition: () =>
    invoke<{ x: number; y: number }>("get_main_panel_position"),
  setMainPanelPosition: (x: number, y: number) =>
    invoke<void>("set_main_panel_position", { x, y }),
  saveMainPanelPosition: () => invoke<void>("save_main_panel_position"),
  showMainPanel: () => invoke<void>("show_main_panel_window"),
  exportTodosBackup: (path: string) =>
    invoke<void>("export_todos_backup", { path }),
  importTodosBackup: (path: string) =>
    invoke<TodoItem[]>("import_todos_backup", { path }),
  saveMarkdownImage: (dataUrl: string, mimeType: string) =>
    invoke<string>("save_markdown_image", { dataUrl, mimeType }),
  debugLog: (scope: string, message: string) =>
    invoke<void>("debug_log", { scope, message }),
  openMainPanelDevtools: () => invoke<void>("open_main_panel_devtools"),
  isMainPanelDevtoolsOpen: () => invoke<boolean>("is_main_panel_devtools_open"),
  getClipboardHistory: (query?: string, limit?: number, offset?: number) =>
    invoke<ClipboardHistoryPage>("get_clipboard_history", { query, limit, offset }),
  deleteClipboardEntry: (id: number) =>
    invoke<void>("delete_clipboard_history_entry", { id }),
  clearClipboardHistory: () => invoke<number>("clear_clipboard_history_command"),
  pinClipboardEntry: (id: number, pinned: boolean) =>
    invoke<ClipboardEntry>("pin_clipboard_history_entry", { id, pinned }),
  copyTextToClipboard: (text: string) => invoke<void>("copy_text_to_clipboard", { text }),
  copyClipboardEntry: (id: number) => invoke<void>("copy_clipboard_entry", { id }),
  getMainPanelClipboardSeed: () =>
    invoke<MainPanelClipboardSeed | null>("get_main_panel_clipboard_seed"),
  seedMainPanelFromSystemClipboard: () =>
    invoke<MainPanelClipboardSeed | null>("seed_main_panel_from_system_clipboard"),
  clearMainPanelClipboardSeed: () => invoke<void>("clear_main_panel_clipboard_seed"),
  getSnippets: (query?: string, groupId?: number | null, sort?: string) =>
    invoke<Snippet[]>("get_snippets", { query, groupId, sort }),
  getSnippetGroups: () => invoke<SnippetGroup[]>("get_snippet_groups"),
  createSnippetGroup: (name: string, color?: string | null) =>
    invoke<SnippetGroup>("create_snippet_group", { name, color }),
  updateSnippetGroup: (id: number, name: string, color?: string | null) =>
    invoke<SnippetGroup>("update_snippet_group_command", { id, name, color }),
  deleteSnippetGroup: (id: number) => invoke<void>("delete_snippet_group_command", { id }),
  createSnippet: (
    title: string,
    content: string,
    tags: string[] = [],
    groupId?: number | null,
    shortcut?: string | null,
    language?: string | null
  ) => invoke<Snippet>("create_snippet", { title, content, tags, groupId, shortcut, language }),
  updateSnippet: (
    id: number,
    title: string,
    content: string,
    tags: string[] = [],
    groupId?: number | null,
    shortcut?: string | null,
    language?: string | null
  ) =>
    invoke<Snippet>("update_snippet_command", {
      id,
      title,
      content,
      tags,
      groupId,
      shortcut,
      language,
    }),
  duplicateSnippet: (id: number) => invoke<Snippet>("duplicate_snippet_command", { id }),
  pinSnippet: (id: number, pinned: boolean) =>
    invoke<Snippet>("pin_snippet_command", { id, pinned }),
  deleteSnippet: (id: number) => invoke<void>("delete_snippet_command", { id }),
  copySnippetToClipboard: (id: number) => invoke<Snippet>("copy_snippet_to_clipboard", { id }),
  showClipboardPicker: () => invoke<void>("show_clipboard_picker"),
  showSnippetPicker: () => invoke<void>("show_snippet_picker"),
  hideShelfPicker: () => invoke<void>("hide_shelf_picker"),
  showLauncherContextMenu: (args: {
    x: number;
    y: number;
    items: Array<{
      id: string;
      label: string;
      disabled?: boolean;
      danger?: boolean;
      separatorBefore?: boolean;
    }>;
    target: unknown;
  }) => invoke<void>("show_launcher_context_menu", { args }),
  hideLauncherContextMenu: (reason?: string) =>
    invoke<void>("hide_launcher_context_menu", { reason: reason ?? null }),
  launcherContextMenuAction: (actionId: string, target: unknown) =>
    invoke<void>("launcher_context_menu_action", { actionId, target }),

  // Plugins
  getPluginRuntimeStatus: () =>
    invoke<PluginRuntimeStatus>("plugin_runtime_status"),
  installPluginRuntime: () =>
    invoke<PluginRuntimeStatus>("plugin_runtime_install"),
  uninstallPluginRuntime: () =>
    invoke<PluginRuntimeStatus>("plugin_runtime_uninstall"),
  listPlugins: () => invoke<InstalledPlugin[]>("list_plugins"),
  importLocalPlugin: (path: string) =>
    invoke<InstalledPackage>("import_local_plugin", { path }),
  trustPlugin: (pluginId: string, version: string, trusted: boolean) =>
    invoke<void>("trust_plugin", {
      args: { pluginId, version, trusted: Boolean(trusted) },
    }),
  setPluginEnabled: (pluginId: string, enabled: boolean) =>
    invoke<void>("set_plugin_enabled_command", {
      args: { pluginId, enabled: Boolean(enabled) },
    }),
  listPluginContributions: () =>
    invoke<PluginContributionBundle[]>("list_plugin_contributions"),
  pluginCallCommand: (pluginId: string, commandId: string, params?: unknown) =>
    invoke<unknown>("plugin_call_command", {
      args: { pluginId, commandId, params: params ?? null },
    }),
  pluginBridgeInvoke: (args: {
    pluginId: string;
    viewInstanceId?: string | null;
    method: string;
    params?: unknown;
  }) =>
    invoke<unknown>("plugin_bridge_invoke", {
      args: {
        pluginId: args.pluginId,
        viewInstanceId: args.viewInstanceId ?? null,
        method: args.method,
        params: args.params ?? null,
      },
    }),
  pluginUiPrepare: (args: {
    pluginId: string;
    appId: string;
    params?: unknown;
    sessionPayload?: unknown;
  }) =>
    invoke<PluginUiPrepareResult>("plugin_ui_prepare", {
      args: {
        pluginId: args.pluginId,
        appId: args.appId,
        params: args.params ?? null,
        sessionPayload: args.sessionPayload ?? null,
      },
    }),
  pluginUiDispose: (viewInstanceId: string) =>
    invoke<void>("plugin_ui_dispose", { viewInstanceId }),
  pluginUiSerializeSession: (viewInstanceId: string) =>
    invoke<void>("plugin_ui_serialize_session", { viewInstanceId }),
  openPluginWindow: (args: {
    pluginId: string;
    appId: string;
    params?: unknown;
  }) =>
    invoke<void>("open_plugin_window", {
      args: {
        pluginId: args.pluginId,
        appId: args.appId,
        params: args.params ?? null,
      },
    }),
  pluginWindowContext: () => invoke<PluginWindowContext>("plugin_window_context"),
  pluginOpenDataDir: (pluginId: string) => invoke<void>("plugin_open_data_dir", { pluginId }),
  revealPluginInstallDir: (pluginId: string) =>
    invoke<void>("reveal_plugin_install_dir", { pluginId }),
  pluginUninstall: (pluginId: string, deleteData: boolean) =>
    invoke<void>("plugin_uninstall", {
      args: { pluginId, deleteData: Boolean(deleteData) },
    }),
  setPluginMcpExposed: (pluginId: string, exposed: boolean) =>
    invoke<void>("set_plugin_mcp_exposed", {
      args: { pluginId, exposed: Boolean(exposed) },
    }),
  setPluginMcpToolEnabled: (pluginId: string, toolName: string, enabled: boolean) =>
    invoke<void>("set_plugin_mcp_tool_enabled", {
      args: { pluginId, toolName, enabled: Boolean(enabled) },
    }),
  listPluginMcpTools: (pluginId: string) =>
    invoke<PluginMcpToolInfo[]>("list_plugin_mcp_tools", { pluginId }),
  promotePluginPendingVersion: (pluginId: string) =>
    invoke<string>("promote_plugin_pending_version", { pluginId }),

  getPluginSettingsBundle: (pluginId: string) =>
    invoke<PluginSettingsBundle>("get_plugin_settings_bundle", { pluginId }),
  setPluginSettingsValues: (pluginId: string, values: Record<string, unknown>) =>
    invoke<Record<string, unknown>>("set_plugin_settings_values", {
      args: { pluginId, values },
    }),

  // Plugin development assistant
  listPluginDevProjects: () =>
    invoke<PluginDevProject[]>("plugin_dev_list_projects"),
  createPluginDevProject: (args: {
    rootPath: string;
    pluginId: string;
    name: string;
    kind: "ui" | "headless" | "hybrid";
  }) => invoke<PluginDevProjectDetail>("plugin_dev_create_project", { args }),
  openPluginDevProject: (rootPath: string) =>
    invoke<PluginDevProjectDetail>("plugin_dev_open_project", {
      args: { rootPath },
    }),
  getPluginDevProject: (projectId: string) =>
    invoke<PluginDevProjectDetail>("plugin_dev_get_project", {
      args: { projectId },
    }),
  writePluginDevManifest: (projectId: string, raw: string, expectedHash: string) =>
    invoke<PluginDevProjectDetail>("plugin_dev_write_manifest", {
      args: { projectId, raw, expectedHash },
    }),
  updatePluginDevPreferences: (
    projectId: string,
    preferences: PluginDevPreferences
  ) =>
    invoke<PluginDevProjectDetail>("plugin_dev_update_preferences", {
      args: { projectId, preferences },
    }),
  probePluginDevUiUrl: (url: string) =>
    invoke<PluginDevProbeResult>("plugin_dev_probe_ui_url", {
      args: { url },
    }),
  connectPluginDevProject: (projectId: string) =>
    invoke<PluginDevConnectionStatus>("plugin_dev_connect", {
      args: { projectId },
    }),
  reloadPluginDevUi: (pluginId: string) =>
    invoke<void>("plugin_dev_reload_ui", { pluginId }),
  disconnectPluginDevProject: (projectId: string) =>
    invoke<PluginDevConnectionStatus>("plugin_dev_disconnect", {
      args: { projectId },
    }),
  reconnectPluginDevRuntime: (projectId: string) =>
    invoke<PluginDevConnectionStatus>("plugin_dev_reconnect_runtime", {
      args: { projectId },
    }),
  runPluginDevMcpTool: (projectId: string, toolName: string, argumentsValue: unknown) =>
    invoke<unknown>("plugin_dev_run_mcp_tool", {
      args: { projectId, toolName, arguments: argumentsValue },
    }),
  forgetPluginDevProject: (projectId: string) =>
    invoke<void>("plugin_dev_forget_project", {
      args: { projectId },
    }),
  listBuiltinMcpTools: (appId: string) =>
    invoke<PluginMcpToolInfo[]>("list_builtin_mcp_tools", { appId }),
  getBuiltinMcpStatus: (appId: string) =>
    invoke<BuiltinMcpStatus>("get_builtin_mcp_status", { appId }),
  setBuiltinMcpExposed: (appId: string, exposed: boolean) =>
    invoke<void>("set_builtin_mcp_exposed", {
      args: { appId, exposed: Boolean(exposed) },
    }),
  setBuiltinMcpToolEnabled: (appId: string, toolName: string, enabled: boolean) =>
    invoke<void>("set_builtin_mcp_tool_enabled", {
      args: { appId, toolName, enabled: Boolean(enabled) },
    }),
  builtinOpenDataDir: (appId: string) => invoke<void>("builtin_open_data_dir", { appId }),

  // Tools — Hosts
  getHostsWorkspace: () => invoke<HostsWorkspace>("get_hosts_workspace"),
  authorizeHostsWrite: () => invoke<HostsWorkspace>("authorize_hosts_write"),
  saveHostsProfile: (args: {
    id?: string | null;
    name: string;
    kind?: "local" | "remote";
    content?: string | null;
    remoteUrl?: string | null;
    refreshIntervalSecs?: number | null;
    importPath?: string | null;
  }) => invoke<HostsProfile>("save_hosts_profile", { args }),
  deleteHostsProfile: (id: string) => invoke<HostsWorkspace>("delete_hosts_profile", { id }),
  setHostsProfileActive: (id: string, active: boolean) =>
    invoke<HostsWorkspace>("set_hosts_profile_active", { id, active }),
  getHostsProfileContent: (id: string) => invoke<string>("get_hosts_profile_content", { id }),
  openHostsFileLocation: () => invoke<void>("open_hosts_file_location"),
  refreshHostsRemoteProfile: (id: string) =>
    invoke<HostsWorkspace>("refresh_hosts_remote_profile", { id }),
  applyHosts: () => invoke<HostsWorkspace>("apply_hosts"),
  flushDns: () => invoke<void>("flush_dns"),
  listHostsBackups: () => invoke<HostsBackup[]>("list_hosts_backups"),
  restoreHostsBackup: (id: string) => invoke<HostsWorkspace>("restore_hosts_backup", { id }),

  // Tools - Port manager
  getPortRecords: (includeActiveConnections = false) =>
    invoke<PortRecord[]>("get_port_records", { includeActiveConnections }),
  terminatePortProcess: (request: TerminatePortProcessRequest) =>
    invoke<void>("terminate_port_process", { request }),

  // Tools — File search
  fileSearchStatus: () => invoke<FileSearchStatus>("file_search_status"),
  fileSearchEnsureEngine: () => invoke<FileSearchStatus>("file_search_ensure_engine"),
  fileSearchQuery: (request: {
    query: string;
    category?: string;
    sort?: string;
    limit?: number;
    offset?: number;
  }) => invoke<FileSearchQueryResult>("file_search_query", { request }),
  fileSearchOpen: (path: string) => invoke<void>("file_search_open", { path }),
  fileSearchReveal: (path: string) => invoke<void>("file_search_reveal", { path }),
  fileSearchPreviewMeta: (path: string) =>
    invoke<FileSearchPreviewMeta>("file_search_preview_meta", { path }),
  fileSearchPreviewUrl: (path: string) =>
    invoke<string>("file_search_preview_url", { path }),
  fileSearchListArchive: (path: string) =>
    invoke<FileSearchArchiveListing>("file_search_list_archive", { path }),

  // Tools — Translate
  getTranslateConfig: () => invoke<TranslateConfig>("get_translate_config"),
  updateTranslateConfig: (config: TranslateConfig) =>
    invoke<TranslateConfig>("update_translate_config", { config }),
  translateText: (provider: string, text: string, from: string, to: string) =>
    invoke<TranslateResult>("translate_text", { provider, text, from, to }),
  translateTextStream: (
    provider: string,
    text: string,
    from: string,
    to: string,
    onEvent: (event: TranslateStreamEvent) => void,
  ) => {
    const channel = new Channel<TranslateStreamEvent>();
    channel.onmessage = onEvent;
    return invoke<TranslateResult>("translate_text_stream", {
      provider,
      text,
      from,
      to,
      onEvent: channel,
    });
  },
  translateCompare: (providers: string[], text: string, from: string, to: string) =>
    invoke<TranslateResult[]>("translate_compare", { providers, text, from, to }),
  testTranslateProvider: (provider: string) =>
    invoke<TranslateResult>("test_translate_provider", { provider }),
};
