import { getHostGlobal } from "./globals.js";
import type {
  AnyPluginIpcContract,
  IpcMainApi,
  PluginIpcContract,
  TempoDisposer,
  TempoLifecycleRegistrar,
  TempoRuntimeApi,
  TempoRuntimeContext,
  TempoRuntimeIpc,
  TempoRuntimeSetup,
} from "./types.js";

export type * from "./types.js";

function createIpc<TContract extends PluginIpcContract>(
  ipcMain: IpcMainApi,
): TempoRuntimeIpc<TContract> {
  return {
    handle: (channel: string, handler: (...args: any[]) => any) =>
      ipcMain.handle(channel, handler as never),
    on: (channel: string, listener: (...args: any[]) => void) =>
      ipcMain.on(channel, listener as never),
    send: (channel: string, ...args: any[]) => ipcMain.send(channel, ...args),
  } as TempoRuntimeIpc<TContract>;
}

export function defineRuntime<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
>(setup: TempoRuntimeSetup<TContract>): void {
  if (typeof setup !== "function") {
    throw new TypeError("defineRuntime requires a setup function");
  }

  const host = getHostGlobal<TempoRuntimeApi>("tempo");
  const ipcMain = getHostGlobal<IpcMainApi>("ipcMain");
  const onMounted = getHostGlobal<TempoLifecycleRegistrar>("onMounted");
  const onUnmounted = getHostGlobal<TempoLifecycleRegistrar>("onUnmounted");
  const disposers: TempoDisposer[] = [];
  let disposing = false;

  async function runDisposers(): Promise<unknown[]> {
    disposing = true;
    const errors: unknown[] = [];
    while (disposers.length > 0) {
      try {
        await disposers.pop()?.();
      } catch (error) {
        errors.push(error);
      }
    }
    return errors;
  }

  const context: TempoRuntimeContext<TContract> = Object.freeze({
    pluginId: host.pluginId,
    paths: host.paths,
    runtime: host.runtime,
    commands: host.commands,
    mcpTools: host.mcpTools,
    events: host.events,
    storage: host.storage,
    files: host.files,
    settings: host.settings,
    notify: host.notify,
    theme: host.theme,
    mainPanel: host.mainPanel,
    apps: host.app,
    external: host.external,
    ipc: createIpc<TContract>(ipcMain),
    onDispose(disposer: TempoDisposer) {
      if (typeof disposer !== "function") {
        throw new TypeError("onDispose requires a function");
      }
      if (disposing) {
        throw new Error("Cannot register a disposer while Runtime is stopping");
      }
      disposers.push(disposer);
    },
  });

  onMounted(async () => {
    try {
      const disposer = await setup(context);
      if (typeof disposer === "function") {
        disposers.push(disposer);
      } else if (disposer !== undefined) {
        throw new TypeError("Runtime setup must return a disposer or nothing");
      }
    } catch (setupError) {
      const cleanupErrors = await runDisposers();
      if (cleanupErrors.length > 0) {
        throw new AggregateError(
          [setupError, ...cleanupErrors],
          "Tempo Runtime setup and cleanup failed",
        );
      }
      throw setupError;
    }
  });

  onUnmounted(async () => {
    const errors = await runDisposers();
    if (errors.length > 0) {
      throw new AggregateError(errors, "Tempo Runtime cleanup failed");
    }
  });
}
