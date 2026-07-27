import type { ThemeMode, Unsubscribe, WindowRectInput } from "./types.js";

export interface NotifyApi {
  show(options?: { title?: string; body?: string }): Promise<void>;
}

export interface ThemeApi {
  get(): Promise<ThemeMode>;
  /**
   * UI-only convenience: registers `theme.onChange`, listens for `theme.changed`,
   * and releases the host subscription on unsubscribe.
   */
  subscribe(handler: (theme: ThemeMode) => void): Promise<Unsubscribe>;
}

export interface MainPanelApi {
  hide(): Promise<void>;
  /** UI only. */
  back(): Promise<void>;
  /** UI only. */
  setSize(height: number): Promise<void>;
}

export interface WindowApi {
  /** Standalone window UI only. */
  setRect(rect: WindowRectInput): Promise<void>;
  /** Standalone window UI only. */
  close(): Promise<void>;
}

export interface AppApi {
  open(appId: string, params?: Record<string, unknown>): Promise<void>;
}

export interface ExternalApi {
  open(url: string): Promise<void>;
}

export interface SessionApi {
  /** UI only. */
  push(payload: unknown): Promise<void>;
}

export type HostCall = (method: string, params?: unknown) => Promise<unknown>;

export function createNotifyApi(call: HostCall): NotifyApi {
  return {
    show(options) {
      return call("notify.show", options ?? {}).then(() => undefined);
    },
  };
}

export function createAppApi(call: HostCall): AppApi {
  return {
    open(appId, params) {
      return call("app.open", { appId, params: params ?? null }).then(() => undefined);
    },
  };
}

export function createExternalApi(call: HostCall): ExternalApi {
  return {
    open(url) {
      return call("external.open", { url }).then(() => undefined);
    },
  };
}

export function createMainPanelApi(call: HostCall, options?: { ui?: boolean }): MainPanelApi {
  const ui = options?.ui ?? false;
  return {
    hide() {
      return call("mainPanel.hide", {}).then(() => undefined);
    },
    back() {
      if (!ui) {
        return Promise.reject(new Error("mainPanel.back is only available in plugin UI"));
      }
      return call("mainPanel.back", {}).then(() => undefined);
    },
    setSize(height) {
      if (!ui) {
        return Promise.reject(new Error("mainPanel.setSize is only available in plugin UI"));
      }
      return call("mainPanel.setSize", { height }).then(() => undefined);
    },
  };
}

export function createWindowApi(call: HostCall): WindowApi {
  return {
    setRect(rect) {
      return call("window.setRect", rect).then(() => undefined);
    },
    close() {
      return call("window.close", {}).then(() => undefined);
    },
  };
}

export function createSessionApi(call: HostCall): SessionApi {
  return {
    push(payload) {
      return call("session.push", { payload }).then(() => undefined);
    },
  };
}

export function createThemeApi(
  call: HostCall,
  onEvent: (event: string, handler: (payload: unknown) => void) => Unsubscribe,
  options?: { ui?: boolean }
): ThemeApi {
  const ui = options?.ui ?? false;
  return {
    async get() {
      const result = (await call("theme.get", {})) as { theme?: ThemeMode } | ThemeMode;
      if (result && typeof result === "object" && "theme" in result) {
        return result.theme ?? "system";
      }
      return (result as ThemeMode) ?? "system";
    },
    async subscribe(handler) {
      if (!ui) {
        throw new Error("theme.subscribe is only available in plugin UI");
      }
      const result = (await call("theme.onChange", {})) as { subscriptionId?: string };
      const subscriptionId = result?.subscriptionId;
      if (!subscriptionId) {
        throw new Error("theme.onChange did not return subscriptionId");
      }
      const off = onEvent("theme.changed", (payload) => {
        const theme =
          payload && typeof payload === "object" && payload !== null && "theme" in payload
            ? String((payload as { theme: unknown }).theme)
            : "system";
        handler(theme);
      });
      return () => {
        off();
        void call("subscription.release", { subscriptionId });
      };
    },
  };
}
