import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, LoaderCircle } from "lucide-react";
import { useOptionalAppNavigation } from "@/apps/navigation";
import { api } from "@/lib/api";
import type { PluginRpcError, PluginUiPrepareResult } from "@/types";

/**
 * Message protocol between the plugin UI document (loaded from `tempo-plugin://`) and this
 * host component, carried over `postMessage` — the plugin never gets a Tauri IPC channel of
 * its own (design §5.3 "UI Bridge is a host data boundary"). Every `host.*`/`runtime.*` call
 * is relayed through the `plugin_bridge_invoke` command, which re-checks `viewInstanceId`
 * ownership on the Rust side; nothing here is trusted at face value from the iframe.
 *
 * The host injects `__tempo__/client.js` into every plugin HTML page and mounts `window.plugin`
 * so plugin authors do not need an SDK. Runtime and host are separate entry points (no name clash):
 *
 *   await window.plugin.invoke("hello", { who: "Tempo" })       // Runtime
 *   await window.plugin.host("notify.show", { title: "Hi" })    // Host
 *
 * Host -> plugin:
 *   { type: "tempo-plugin-context", apiVersion, theme, params, session }
 *   { type: "tempo-plugin-rpc-response", id, ok, result | error }
 *   { type: "tempo-plugin-event", subscriptionId?, event, payload }
 *
 * Plugin -> host (via window.plugin.invoke / plugin.host → postMessage):
 *   { type: "tempo-plugin-rpc", id, method, params }
 */
type HostToPluginMessage =
  | { type: "tempo-plugin-context"; apiVersion: string; theme: string; params: unknown; session: unknown }
  | { type: "tempo-plugin-rpc-response"; id: string; ok: true; result: unknown }
  | { type: "tempo-plugin-rpc-response"; id: string; ok: false; error: PluginRpcError }
  | { type: "tempo-plugin-event"; subscriptionId?: string; event: string; payload: unknown };

interface PluginToHostRpcMessage {
  type: "tempo-plugin-rpc";
  id: string;
  method: string;
  params?: unknown;
}

function isPluginRpcMessage(data: unknown): data is PluginToHostRpcMessage {
  return (
    Boolean(data) &&
    typeof data === "object" &&
    (data as { type?: unknown }).type === "tempo-plugin-rpc" &&
    typeof (data as { id?: unknown }).id === "string" &&
    typeof (data as { method?: unknown }).method === "string"
  );
}

function normalizeRpcError(error: unknown): PluginRpcError {
  if (
    error &&
    typeof error === "object" &&
    typeof (error as { code?: unknown }).code === "string" &&
    typeof (error as { message?: unknown }).message === "string"
  ) {
    return error as PluginRpcError;
  }
  return { code: "INTERNAL", message: error instanceof Error ? error.message : String(error) };
}

export interface PluginAppHostProps {
  pluginId?: string;
  /**
   * Manifest-local app id (not the `{pluginId}/{localId}` runtime id). Accepts either prop
   * name — `localAppId` and `appId` are both in use across call sites.
   */
  localAppId?: string;
  appId?: string;
  params?: Record<string, unknown>;
  /** Defaults to true: ask the UI to serialize its session before disposing the view. */
  persistSession?: boolean;
  /**
   * Called when the plugin asks the host to go back (Esc / `host.palette.back`). Falls back
   * to the ambient `AppNavigationProvider` context when omitted.
   */
  onBack?: () => void;
}

/**
 * Loads a plugin's UI entry inside an `<iframe>` served by the `tempo-plugin://` protocol and
 * bridges it to the Rust Host Bridge via `postMessage` <-> `plugin_bridge_invoke` (task
 * architecture decision: iframe + postMessage rather than a raw Wry child WebView).
 */
export function PluginAppHost({
  pluginId,
  localAppId,
  appId,
  params,
  persistSession = true,
  onBack,
}: PluginAppHostProps) {
  const navigation = useOptionalAppNavigation();
  const resolvedAppId = localAppId ?? appId ?? "";
  const goBack = onBack ?? navigation?.backToSearch ?? (() => undefined);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [prepared, setPrepared] = useState<PluginUiPrepareResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const originRef = useRef<string | null>(null);
  const viewInstanceIdRef = useRef<string | null>(null);

  const postToPlugin = useCallback((message: HostToPluginMessage) => {
    const win = iframeRef.current?.contentWindow;
    if (!win) return;
    win.postMessage(message, originRef.current ?? "*");
  }, []);

  useEffect(() => {
    if (!pluginId) {
      setError("插件应用缺少 pluginId");
      return;
    }
    let disposed = false;
    setPrepared(null);
    setError(null);

    api
      .pluginUiPrepare({ pluginId, appId: resolvedAppId, params })
      .then((result) => {
        if (disposed) {
          void api.pluginUiDispose(result.viewInstanceId);
          return;
        }
        viewInstanceIdRef.current = result.viewInstanceId;
        try {
          originRef.current = new URL(result.entryUrl).origin;
        } catch {
          originRef.current = null;
        }
        setPrepared(result);
      })
      .catch((prepareError) => {
        if (disposed) return;
        setError(prepareError instanceof Error ? prepareError.message : String(prepareError));
      });

    return () => {
      disposed = true;
      const viewInstanceId = viewInstanceIdRef.current;
      viewInstanceIdRef.current = null;
      originRef.current = null;
      if (!viewInstanceId) return;
      const dispose = () => void api.pluginUiDispose(viewInstanceId).catch(() => undefined);
      if (persistSession) {
        // Ask the UI for a last session snapshot before tearing the view down — dispose runs
        // after serialize resolves, since the host clears the view's session cache on dispose
        // (design §5.5). This is fire-and-forget from the caller's perspective: the component
        // is already gone from the tree by the time this settles.
        void api.pluginUiSerializeSession(viewInstanceId).catch(() => undefined).finally(dispose);
      } else {
        dispose();
      }
    };
    // `params` is intentionally captured only at open time (matches builtin app semantics).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pluginId, resolvedAppId]);

  useEffect(() => {
    if (!prepared || !pluginId) return;
    const viewInstanceId = prepared.viewInstanceId;
    const resolvedPluginId = pluginId;

    function onMessage(event: MessageEvent) {
      if (event.source !== iframeRef.current?.contentWindow) return;
      if (originRef.current && event.origin !== originRef.current) return;
      if (!isPluginRpcMessage(event.data)) return;

      const { id, method, params: rpcParams } = event.data;
      if (!resolvedPluginId) return;
      api
        .pluginBridgeInvoke({ pluginId: resolvedPluginId, viewInstanceId, method, params: rpcParams })
        .then((result) => postToPlugin({ type: "tempo-plugin-rpc-response", id, ok: true, result }))
        .catch((rpcError) =>
          postToPlugin({
            type: "tempo-plugin-rpc-response",
            id,
            ok: false,
            error: normalizeRpcError(rpcError),
          })
        );
    }

    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [prepared, pluginId, postToPlugin]);

  useEffect(() => {
    if (!prepared || !pluginId) return;
    const viewInstanceId = prepared.viewInstanceId;

    const unlistenBack = listen<{ viewInstanceId: string }>("plugin-host:palette-back", (e) => {
      if (e.payload.viewInstanceId === viewInstanceId) goBack();
    });
    const unlistenEvent = listen<{
      pluginId: string;
      viewInstanceId?: string;
      subscriptionId?: string;
      event: string;
      payload: unknown;
    }>("plugin-runtime-event", (e) => {
      if (e.payload.pluginId !== pluginId) return;
      if (e.payload.viewInstanceId && e.payload.viewInstanceId !== viewInstanceId) return;
      postToPlugin({
        type: "tempo-plugin-event",
        subscriptionId: e.payload.subscriptionId,
        event: e.payload.event,
        payload: e.payload.payload,
      });
    });

    return () => {
      void unlistenBack.then((fn) => fn());
      void unlistenEvent.then((fn) => fn());
    };
  }, [prepared, pluginId, goBack, postToPlugin]);

  const handleIframeLoad = useCallback(() => {
    if (!prepared) return;
    postToPlugin({
      type: "tempo-plugin-context",
      apiVersion: prepared.apiVersion,
      theme: prepared.theme,
      params: prepared.params,
      session: prepared.session ?? null,
    });
  }, [prepared, postToPlugin]);

  if (error) {
    return (
      <div className="flex h-full min-h-[240px] flex-col items-center justify-center gap-3 px-6 text-center text-muted-foreground">
        <AlertTriangle className="size-8 opacity-60" aria-hidden="true" />
        <div className="space-y-1">
          <p className="text-sm font-medium text-foreground">插件加载失败</p>
          <p className="text-xs leading-relaxed">{error}</p>
        </div>
      </div>
    );
  }

  if (!prepared) {
    return (
      <div className="flex h-full min-h-[240px] items-center justify-center text-muted-foreground">
        <LoaderCircle className="size-6 animate-spin" aria-hidden="true" />
      </div>
    );
  }

  return (
    <iframe
      ref={iframeRef}
      src={prepared.entryUrl}
      title={resolvedAppId}
      className="h-full w-full border-0"
      // Host data boundary: allow scripts/same-origin for the plugin document + forms,
      // but block top-navigation, popups, and privileged device APIs (design §5 / §14 note).
      sandbox="allow-scripts allow-same-origin allow-forms"
      onLoad={handleIframeLoad}
    />
  );
}
