/**
 * `@tempo/plugin-sdk` — Phase 1 (MVP) type-only SDK for Tempo plugin `main` entries.
 *
 * This package ships **types and small structural helpers only**. It is bundled at build
 * time into a plugin's root `main.mjs` / `main.js` (design §3.3/§10.3) — plugin authors never
 * install a matching runtime package on end users' machines, and Tempo never downloads this
 * package for them. There is no code here that talks to a socket, spawns a process, or reads
 * `manifest.json`; that all lives in the host (`src-tauri/src/plugins/*`) and in the tiny
 * `plugin-runtime/bootstrap.mjs` launcher the host writes out and runs for you.
 *
 * See `docs/plugin-system-design.md` for the full design. In particular:
 * - §6.3 "Runtime 执行模型" for `activate`/`deactivate` and `ExtensionContext`.
 * - §7 "Host Bridge API" for the RPC envelope and error codes these types describe.
 */

// ---------------------------------------------------------------------------------------
// RPC envelope (design §7)
// ---------------------------------------------------------------------------------------

/** Structured error codes the host and bootstrap protocol use (design §7). */
export type RpcErrorCode =
  | "INVALID_REQUEST"
  | "PAYLOAD_TOO_LARGE"
  | "RESOURCE_EXHAUSTED"
  | "NOT_FOUND"
  | "FORBIDDEN"
  | "TIMEOUT"
  | "CANCELLED"
  | "ACTIVATION_FAILED"
  | "RUNTIME_UNAVAILABLE"
  | "COMMAND_FAILED"
  | "INTERNAL";

export interface RpcError {
  code: RpcErrorCode;
  message: string;
  data?: unknown;
}

/**
 * Throw this (or a plain `Error` — the bootstrap wraps it) from a command handler to send a
 * `COMMAND_FAILED` response with your own `message`/`data` back to the caller (host UI,
 * action, or a public cross-plugin caller) instead of a scrubbed `INTERNAL` error
 * (design §7, "COMMAND_FAILED 与 INTERNAL 必须严格区分").
 */
export class PluginCommandError extends Error {
  readonly data?: unknown;

  constructor(message: string, data?: unknown) {
    super(message);
    this.name = "PluginCommandError";
    this.data = data;
  }
}

// ---------------------------------------------------------------------------------------
// Commands (design §6.1, §6.3)
// ---------------------------------------------------------------------------------------

/**
 * A registered command handler. Receives the caller's `params` and an `AbortSignal` that
 * fires when the caller cancels or the default 30s command timeout elapses (design §6.3) —
 * long-running handlers should check `signal.aborted` / listen for `"abort"`.
 */
export type CommandHandler<TParams = unknown, TResult = unknown> = (
  params: TParams,
  signal: AbortSignal
) => TResult | Promise<TResult>;

// ---------------------------------------------------------------------------------------
// Host Bridge API surface exposed to `main` as `ctx.host.*` (design §7.1)
// ---------------------------------------------------------------------------------------

export interface HostPaletteApi {
  /** Hide the command palette window (e.g. after a background action finishes). */
  hide(): Promise<void>;
}

export interface HostAppApi {
  /** Open another registered TempoApp (builtin or plugin) by its runtime id. */
  open(appId: string, params?: Record<string, unknown>): Promise<void>;
}

export interface HostExternalApi {
  /** Open a URL with the user's default app, after host-side scheme/policy checks. Only
   * `http(s)://` and `mailto:` are accepted (design §7.1). */
  open(url: string): Promise<void>;
}

export interface HostNotifyApi {
  /** Show a native OS notification. */
  show(options: { title?: string; body?: string }): Promise<void>;
}

export interface HostThemeApi {
  get(): Promise<{ theme: "light" | "dark" | "system" | string }>;
}

export interface HostStoragePluginApi {
  /** Per-plugin private KV (design §7.1): 5 MiB total / 256 KiB per value by default. */
  get<T = unknown>(key: string): Promise<T | null>;
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  list(): Promise<string[]>;
}

export interface HostStorageApi {
  plugin: HostStoragePluginApi;
}

export interface HostApi {
  palette: HostPaletteApi;
  app: HostAppApi;
  external: HostExternalApi;
  notify: HostNotifyApi;
  theme: HostThemeApi;
  storage: HostStorageApi;
}

// ---------------------------------------------------------------------------------------
// ExtensionContext (design §6.3)
// ---------------------------------------------------------------------------------------

export interface UiEventApi {
  /**
   * Broadcast an event to every currently-open UI instance of *this* plugin
   * (`runtime.on(event)` on the UI side). Not persisted or replayed — instances that are not
   * open when this is called never see it (design §5.3).
   */
  emit(event: string, payload?: unknown): void;
}

export interface RuntimeInfo {
  /** The actual on-demand Node version Tempo installed for plugins (design §3.3.1). Fixed by
   * Tempo, not chosen by the plugin. */
  nodeVersion: string;
}

export interface ExtensionPaths {
  /** Absolute path to this plugin's writable data directory (separate from its read-only
   * install directory — safe across updates/rollbacks, design §8.1). */
  data: string;
}

export interface ExtensionContext {
  /** This plugin's package id (`manifest.json#id`), e.g. `"com.example.hello"`. */
  pluginId: string;
  /** Register a command declared in `manifest.json#contributes.commands`. Call once per id
   * during `activate` — there is no `unregisterCommand`; commands live for the Runtime's
   * lifetime and disappear when it is stopped (disable/crash/update). */
  registerCommand<TParams = unknown, TResult = unknown>(
    id: string,
    handler: CommandHandler<TParams, TResult>
  ): void;
  host: HostApi;
  ui: UiEventApi;
  paths: ExtensionPaths;
  runtime: RuntimeInfo;
}

/** A plugin `main` bundle's required shape (design §6.3 / Appendix C). */
export interface PluginModule {
  /** Must complete within 10s and finish registering every command it needs
   * (design §6.3) — partial activation is not supported. */
  activate(ctx: ExtensionContext): void | Promise<void>;
  /** Optional graceful shutdown hook. Not a reliable uninstall path — the Supervisor kills
   * the process tree if this doesn't return promptly (design §6.2). */
  deactivate?(): void | Promise<void>;
}
