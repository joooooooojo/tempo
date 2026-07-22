import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";

export type AppSource = "builtin" | "plugin";

export interface BuiltinAppProps {
  onBack: () => void;
  /** When opening 快捷短语 from shelf "新建", open the create dialog once. */
  openCreateOnMount?: boolean;
  /** Prefill 聚合翻译 source and auto-run translation. */
  initialTranslateText?: string;
}

export interface BuiltinApp {
  id: string;
  name: string;
  keywords: string[];
  icon: LucideIcon;
  component: ComponentType<BuiltinAppProps>;
  source: AppSource;
  defaultSize?: { width?: number; height?: number };
  /**
   * When true, dismissing the palette by clicking outside (blur) keeps this app
   * as the next open target. Esc / explicit back clears the session.
   * Opt-in per app (and future plugins).
   */
  persistSession?: boolean;
}

export interface OpenBuiltinAppOptions {
  createSnippet?: boolean;
  initialTranslateText?: string;
  /** Restoring a persisted session — skip usage bump. */
  restore?: boolean;
}

/** Runtime helpers passed into a quick action when the user runs it. */
export interface QuickActionContext {
  query: string;
  openApp: (appId: string, options?: OpenBuiltinAppOptions) => void;
  hideAndReset: () => Promise<void>;
}

/**
 * Palette quick action (快捷操作). Built-ins and future plugins share this shape.
 * Register via `registerQuickAction` / the actions registry.
 */
export interface QuickAction {
  id: string;
  name: string;
  keywords?: string[];
  icon: LucideIcon;
  source: AppSource;
  /** Default true: only shown / runnable when the search query is non-empty. */
  requiresQuery?: boolean;
  /** Return an error message to block execution / mark the tile invalid. */
  validate?: (query: string) => string | null;
  title?: (query: string) => string;
  run: (ctx: QuickActionContext) => void | Promise<void>;
}
