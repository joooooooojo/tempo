export type ActionInput =
  | { kind: "none" }
  | { kind: "text"; text: string }
  | {
      kind: "image";
      entryId: number;
      imageUrl: string;
      /** Present only when the action targets a Runtime command. */
      filePath?: string;
      width?: number | null;
      height?: number | null;
    };

/** Payload delivered to an action's target app or Runtime command. */
export interface ActionInvocation {
  actionId: string;
  query: string;
  input: ActionInput;
}

export type CommandHandler<TParams = unknown, TResult = unknown> = (
  params: TParams,
  signal: AbortSignal
) => TResult | Promise<TResult>;

export type ThemeMode = "light" | "dark" | "system" | string;

export interface PluginUiContext {
  apiVersion: string;
  theme: ThemeMode;
  params: unknown;
  session: unknown | null;
}

export type WindowLength = number | `${number}%` | "center";

export interface WindowRectInput {
  width?: number | `${number}%`;
  height?: number | `${number}%`;
  x?: WindowLength;
  y?: WindowLength;
}

export type Unsubscribe = () => void;
