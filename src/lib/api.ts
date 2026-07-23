import { invoke } from "@tauri-apps/api/core";
import type {
  AppUsage,
  ClipboardEntry,
  ClipboardHistoryPage,
  CommandPaletteClipboardSeed,
  DailyReport,
  HostsBackup,
  HostsProfile,
  HostsWorkspace,
  InstalledPackage,
  InstalledPlugin,
  LauncherApp,
  LauncherUsageItem,
  PluginContributionBundle,
  PluginMcpToolInfo,
  PluginRuntimeStatus,
  PluginUiPrepareResult,
  PortRecord,
  PomodoroState,
  Settings,
  Snippet,
  SnippetGroup,
  TodoFocusSummary,
  TodoImage,
  TodoItem,
  TodoNote,
  TodoRecurrence,
  TerminatePortProcessRequest,
  TranslateConfig,
  TranslateResult,
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
  launchIndexedApp: (id: string) => invoke<void>("launch_indexed_app", { id }),
  setLauncherAppPinned: (id: string, pinned: boolean) =>
    invoke<void>("set_launcher_app_pinned", { id, pinned }),
  getLauncherUsage: () => invoke<LauncherUsageItem[]>("get_launcher_usage"),
  recordLauncherUsage: (id: string) => invoke<void>("record_launcher_usage", { id }),
  setCommandPaletteHeight: (height: number) =>
    invoke<void>("set_command_palette_height", { height }),
  setCommandPaletteSize: (width: number | null, height: number) =>
    invoke<void>("set_command_palette_size", { width, height }),
  showCommandPalette: () => invoke<void>("show_command_palette_window"),
  exportTodosBackup: (path: string) =>
    invoke<void>("export_todos_backup", { path }),
  importTodosBackup: (path: string) =>
    invoke<TodoItem[]>("import_todos_backup", { path }),
  saveMarkdownImage: (dataUrl: string, mimeType: string) =>
    invoke<string>("save_markdown_image", { dataUrl, mimeType }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  debugLog: (scope: string, message: string) =>
    invoke<void>("debug_log", { scope, message }),
  hideToTray: () => invoke<void>("hide_to_tray_command"),
  showWindow: () => invoke<void>("show_window"),
  getPomodoroState: () => invoke<PomodoroState>("get_pomodoro_state"),
  setPomodoroTodo: (todoId: number | null) =>
    invoke<PomodoroState>("set_pomodoro_todo", { todoId }),
  startPomodoro: (todoId?: number | null) =>
    invoke<PomodoroState>("start_pomodoro", { todoId: todoId ?? null }),
  pausePomodoro: () => invoke<PomodoroState>("pause_pomodoro"),
  stopPomodoro: () => invoke<PomodoroState>("stop_pomodoro"),
  skipPomodoroPhase: () => invoke<PomodoroState>("skip_pomodoro_phase"),
  getTodoFocusSummary: (todoId: number) =>
    invoke<TodoFocusSummary>("get_todo_focus_summary", { todoId }),
  getTodoFocusSummaries: (todoIds: number[]) =>
    invoke<TodoFocusSummary[]>("get_todo_focus_summaries", { todoIds }),
  showPomodoroFloat: () => invoke<void>("show_pomodoro_float"),
  hidePomodoroFloat: () => invoke<void>("hide_pomodoro_float"),
  togglePomodoroFloat: () => invoke<boolean>("toggle_pomodoro_float"),
  isPomodoroFloatVisible: () => invoke<boolean>("is_pomodoro_float_visible_command"),
  setPomodoroFloatExpanded: (expanded: boolean) =>
    invoke<void>("set_pomodoro_float_expanded", { expanded }),
  savePomodoroFloatPosition: (x: number, y: number) =>
    invoke<void>("save_pomodoro_float_position", { x, y }),
  popupPomodoroFloatMenu: () => invoke<void>("popup_pomodoro_float_menu"),
  getClipboardHistory: (query?: string, limit?: number, offset?: number) =>
    invoke<ClipboardHistoryPage>("get_clipboard_history", { query, limit, offset }),
  deleteClipboardEntry: (id: number) =>
    invoke<void>("delete_clipboard_history_entry", { id }),
  clearClipboardHistory: () => invoke<number>("clear_clipboard_history_command"),
  pinClipboardEntry: (id: number, pinned: boolean) =>
    invoke<ClipboardEntry>("pin_clipboard_history_entry", { id, pinned }),
  copyTextToClipboard: (text: string) => invoke<void>("copy_text_to_clipboard", { text }),
  copyClipboardEntry: (id: number) => invoke<void>("copy_clipboard_entry", { id }),
  getCommandPaletteClipboardSeed: () =>
    invoke<CommandPaletteClipboardSeed | null>("get_command_palette_clipboard_seed"),
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
  pluginOpenDataDir: (pluginId: string) => invoke<void>("plugin_open_data_dir", { pluginId }),
  pluginUninstall: (pluginId: string, deleteData: boolean) =>
    invoke<void>("plugin_uninstall", {
      args: { pluginId, deleteData: Boolean(deleteData) },
    }),
  setPluginMcpExposed: (pluginId: string, exposed: boolean) =>
    invoke<void>("set_plugin_mcp_exposed", {
      args: { pluginId, exposed: Boolean(exposed) },
    }),
  listPluginMcpTools: (pluginId: string) =>
    invoke<PluginMcpToolInfo[]>("list_plugin_mcp_tools", { pluginId }),
  promotePluginPendingVersion: (pluginId: string) =>
    invoke<string>("promote_plugin_pending_version", { pluginId }),

  // Tools — Hosts
  getHostsWorkspace: () => invoke<HostsWorkspace>("get_hosts_workspace"),
  authorizeHostsWrite: () => invoke<HostsWorkspace>("authorize_hosts_write"),
  saveHostsPublic: (content: string) => invoke<HostsWorkspace>("save_hosts_public", { content }),
  saveHostsProfile: (name: string, content: string, id?: string | null) =>
    invoke<HostsProfile>("save_hosts_profile", { id: id ?? null, name, content }),
  deleteHostsProfile: (id: string) => invoke<HostsWorkspace>("delete_hosts_profile", { id }),
  activateHostsProfile: (id?: string | null) =>
    invoke<HostsWorkspace>("activate_hosts_profile", { id: id ?? null }),
  getHostsProfileContent: (id: string) => invoke<string>("get_hosts_profile_content", { id }),
  applyHosts: () => invoke<HostsWorkspace>("apply_hosts"),
  flushDns: () => invoke<void>("flush_dns"),
  listHostsBackups: () => invoke<HostsBackup[]>("list_hosts_backups"),
  restoreHostsBackup: (id: string) => invoke<HostsWorkspace>("restore_hosts_backup", { id }),

  // Tools - Port manager
  getPortRecords: (includeActiveConnections = false) =>
    invoke<PortRecord[]>("get_port_records", { includeActiveConnections }),
  terminatePortProcess: (request: TerminatePortProcessRequest) =>
    invoke<void>("terminate_port_process", { request }),

  // Tools — Translate
  getTranslateConfig: () => invoke<TranslateConfig>("get_translate_config"),
  updateTranslateConfig: (config: TranslateConfig) =>
    invoke<TranslateConfig>("update_translate_config", { config }),
  translateText: (provider: string, text: string, from: string, to: string) =>
    invoke<TranslateResult>("translate_text", { provider, text, from, to }),
  translateCompare: (providers: string[], text: string, from: string, to: string) =>
    invoke<TranslateResult[]>("translate_compare", { providers, text, from, to }),
  testTranslateProvider: (provider: string) =>
    invoke<TranslateResult>("test_translate_provider", { provider }),
};
