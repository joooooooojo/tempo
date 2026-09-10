import type {
  IpcMainApi,
  IpcRendererApi,
  TempoLifecycleRegistrar,
  TempoRuntimeApi,
  TempoUiApi,
} from "./types.js";

const TRANSPORT_PROTOCOL_VERSION = 1;

type TempoUiTransport = {
  protocolVersion: number;
  tempo: TempoUiApi;
  ipcRenderer: IpcRendererApi;
};

type TempoRuntimeTransport = {
  protocolVersion: number;
  tempo: TempoRuntimeApi;
  ipcMain: IpcMainApi;
  onMounted: TempoLifecycleRegistrar;
  onUnmounted: TempoLifecycleRegistrar;
};

function getHostGlobal<T>(name: string): T {
  const value = (globalThis as Record<string, unknown>)[name];
  if (value === undefined || value === null) {
    throw new Error(
      `Tempo SDK could not connect: host global \"${name}\" is unavailable`,
    );
  }
  return value as T;
}

function getInternalTransport<T>(name: string): T | undefined {
  const value = (globalThis as Record<string, unknown>)[name];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "object") {
    throw new Error(`Tempo SDK could not connect: host transport "${name}" is invalid`);
  }
  const protocolVersion = (value as { protocolVersion?: unknown }).protocolVersion;
  if (protocolVersion !== TRANSPORT_PROTOCOL_VERSION) {
    throw new Error(
      `Tempo SDK could not connect: host transport "${name}" uses unsupported protocol ${String(protocolVersion)} (expected ${TRANSPORT_PROTOCOL_VERSION})`,
    );
  }
  return value as T;
}

export function getUiTransport(): TempoUiTransport {
  return (
    getInternalTransport<TempoUiTransport>("__tempoPluginUi") ?? {
      protocolVersion: TRANSPORT_PROTOCOL_VERSION,
      tempo: getHostGlobal<TempoUiApi>("tempo"),
      ipcRenderer: getHostGlobal<IpcRendererApi>("ipcRenderer"),
    }
  );
}

export function getRuntimeTransport(): TempoRuntimeTransport {
  return (
    getInternalTransport<TempoRuntimeTransport>("__tempoPluginRuntime") ?? {
      protocolVersion: TRANSPORT_PROTOCOL_VERSION,
      tempo: getHostGlobal<TempoRuntimeApi>("tempo"),
      ipcMain: getHostGlobal<IpcMainApi>("ipcMain"),
      onMounted: getHostGlobal<TempoLifecycleRegistrar>("onMounted"),
      onUnmounted: getHostGlobal<TempoLifecycleRegistrar>("onUnmounted"),
    }
  );
}
