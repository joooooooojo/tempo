import { hostGlobal } from "./globals.js";
import type {
  IpcMainApi,
  TempoLifecycleRegistrar,
  TempoRuntimeApi,
} from "./types.js";

export type * from "./types.js";

export const tempo = hostGlobal<TempoRuntimeApi>("tempo");
export const ipcMain = hostGlobal<IpcMainApi>("ipcMain");
export const onMounted = hostGlobal<TempoLifecycleRegistrar>("onMounted");
export const onUnmounted = hostGlobal<TempoLifecycleRegistrar>("onUnmounted");
