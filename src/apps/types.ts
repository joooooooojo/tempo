import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";

export type AppSource = "builtin" | "plugin";
export type AppWindowMode = "normal" | "standalone";
export type AppRectValue = number | string;

export interface AppRect {
  width?: AppRectValue;
  height?: AppRectValue;
  x?: AppRectValue;
  y?: AppRectValue;
}

/** Icon for main panel tiles. Plugin SVG/PNG must never be inlined into the host DOM. */
export type AppIconDescriptor =
  | { type: "lucide"; icon: LucideIcon }
  | { type: "file"; path: string; url?: string };

export interface TempoAppProps {
  onBack: () => void;
  /** Generic open params (plugins + future builtins). */
  params?: Record<string, unknown>;
  /** When opening 快捷短语 from shelf "新建", open the create dialog once. */
  openCreateOnMount?: boolean;
  /** Prefill 聚合翻译 source and auto-run translation. */
  initialTranslateText?: string;
}

export type TempoAppUi =
  | { type: "react"; component: ComponentType<TempoAppProps> }
  | { type: "plugin-webview"; entryPath: string; localAppId: string };

export interface TempoApp {
  /** Runtime id: builtin uses local id; plugins use `{pluginId}/{appId}`. */
  id: string;
  name: string;
  keywords: string[];
  icon: AppIconDescriptor;
  source: AppSource;
  pluginId?: string;
  /** True when the active provider comes from the built-in development assistant. */
  development?: boolean;
  /** UI source selected for a plugin connected through the development assistant. */
  developmentUiSource?: "url" | "static";
  windowMode?: AppWindowMode;
  rect?: AppRect;
  ui: TempoAppUi;
}

export interface OpenAppOptions {
  /** Restoring a persisted session — skip usage bump. */
  restore?: boolean;
  params?: Record<string, unknown>;
  /** Convenience for snippets; also mirrored into params when set. */
  createSnippet?: boolean;
  /** Convenience for translate; also mirrored into params when set. */
  initialTranslateText?: string;
}

/** @deprecated Prefer TempoAppProps */
export type BuiltinAppProps = TempoAppProps;
/** @deprecated Prefer TempoApp */
export type BuiltinApp = TempoApp;
/** @deprecated Prefer OpenAppOptions */
export type OpenBuiltinAppOptions = OpenAppOptions;

/** Input kinds an action can declare in `accepts`. */
export type QuickActionAcceptKind = "text" | "image" | "file";

/** Runtime main-panel input, including the empty state. */
export type QuickActionInputKind = "none" | QuickActionAcceptKind;

export type QuickActionInput =
  | { kind: "none" }
  | { kind: "text"; text: string }
  | {
      kind: "image";
      entryId: number;
      imageUrl: string;
      width?: number | null;
      height?: number | null;
    }
  | {
      kind: "file";
      entryId: number;
      paths: string[];
    };

/** Runtime helpers passed into a quick action when the user runs it. */
export interface QuickActionContext {
  /** Search text kept for compatibility and action-title interpolation. */
  query: string;
  /** The actual main panel input consumed by this action. */
  input: QuickActionInput;
  openApp: (appId: string, options?: OpenAppOptions) => void;
  hideAndReset: () => Promise<void>;
}

/**
 * Main panel quick action (快捷操作). Built-ins and plugins share this shape.
 * Register via `registerQuickAction` / the actions registry.
 */
export interface QuickAction {
  id: string;
  name: string;
  keywords?: string[];
  icon: AppIconDescriptor;
  /** When "app", render like launcher tiles (AppIcon) instead of green builtin quick-action style. */
  iconStyle?: "app" | "builtin";
  /** Display name for AppIcon fallback when `iconStyle` is "app". */
  appIconName?: string;
  source: AppSource;
  pluginId?: string;
  /** Input kinds for which this action is shown and can run. */
  accepts: QuickActionAcceptKind[];
  /**
   * Extra visibility gate after `accepts` matches (e.g. clipboard text is a URL).
   * Return false to hide the tile entirely.
   */
  isVisible?: (input: QuickActionInput) => boolean;
  /**
   * Sort weight for `listVisibleQuickActions` (higher first). Default 0.
   * Use for context-specific actions (e.g. open-link when clipboard is a URL).
   */
  priority?: number;
  /** Return an error message to block execution / mark the tile invalid. */
  validate?: (query: string) => string | null;
  title?: (query: string) => string;
  /** Declarative plugins may set a template instead of a title function. */
  titleTemplate?: string;
  run: (ctx: QuickActionContext) => void | Promise<void>;
}

export interface Registration {
  dispose: () => void;
}

export function lucideIcon(icon: LucideIcon): AppIconDescriptor {
  return { type: "lucide", icon };
}

export function fileIcon(url: string | null | undefined, path = ""): AppIconDescriptor {
  const resolved = url ?? path;
  return { type: "file", path: path || resolved, url: url ?? undefined };
}

export function resolveOpenAppParams(options?: OpenAppOptions): Record<string, unknown> {
  const params: Record<string, unknown> = { ...(options?.params ?? {}) };
  if (options?.createSnippet !== undefined && params.createSnippet === undefined) {
    params.createSnippet = options.createSnippet;
  }
  if (options?.initialTranslateText !== undefined && params.initialTranslateText === undefined) {
    params.initialTranslateText = options.initialTranslateText;
  }
  return params;
}
