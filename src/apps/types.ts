import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";

export type AppSource = "builtin" | "plugin";

/** Icon for palette tiles. Plugin SVG/PNG must never be inlined into the host DOM. */
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
  | { type: "plugin-webview"; entryPath: string };

export interface TempoApp {
  /** Runtime id: builtin uses local id; plugins use `{pluginId}/{appId}`. */
  id: string;
  name: string;
  keywords: string[];
  icon: AppIconDescriptor;
  source: AppSource;
  pluginId?: string;
  defaultSize?: { width?: number; height?: number };
  /**
   * When true, dismissing the palette by clicking outside (blur) keeps this app
   * as the next open target. Esc / explicit back clears the session.
   */
  persistSession?: boolean;
  sessionVersion?: number;
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

/** Runtime helpers passed into a quick action when the user runs it. */
export interface QuickActionContext {
  query: string;
  openApp: (appId: string, options?: OpenAppOptions) => void;
  hideAndReset: () => Promise<void>;
}

/**
 * Palette quick action (快捷操作). Built-ins and plugins share this shape.
 * Register via `registerQuickAction` / the actions registry.
 */
export interface QuickAction {
  id: string;
  name: string;
  keywords?: string[];
  icon: AppIconDescriptor;
  source: AppSource;
  pluginId?: string;
  /** Default true: only shown / runnable when the search query is non-empty. */
  requiresQuery?: boolean;
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
