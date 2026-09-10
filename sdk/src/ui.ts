import { getUiTransport } from "./globals.js";
import type {
  AnyPluginIpcContract,
  IpcRendererApi,
  PluginIpcContract,
  TempoUiClient,
  TempoUiIpc,
} from "./types.js";

export type * from "./types.js";

function createIpc<TContract extends PluginIpcContract>(
  ipcRenderer: IpcRendererApi,
): TempoUiIpc<TContract> {
  return {
    invoke: (channel: string, ...args: any[]) =>
      ipcRenderer.invoke(channel, ...args),
    send: (channel: string, ...args: any[]) =>
      ipcRenderer.send(channel, ...args),
    on: (channel: string, listener: (...args: any[]) => void) =>
      ipcRenderer.on(channel, listener as never),
  } as TempoUiIpc<TContract>;
}

export async function connect<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
>(): Promise<TempoUiClient<TContract>> {
  const { tempo: host, ipcRenderer } = getUiTransport();
  const context = await host.ready();
  if (!context || typeof context !== "object") {
    throw new Error("Tempo SDK could not connect: host returned an invalid UI context");
  }

  return Object.freeze({
    context,
    events: host.events,
    storage: host.storage,
    files: host.files,
    settings: host.settings,
    notify: host.notify,
    theme: host.theme,
    mainPanel: host.mainPanel,
    window: host.window,
    apps: host.app,
    external: host.external,
    session: host.session,
    ipc: createIpc<TContract>(ipcRenderer),
  });
}
