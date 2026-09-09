import { hostGlobal } from "./globals.js";
import type { IpcRendererApi, TempoUiApi } from "./types.js";

export type * from "./types.js";

export const tempo = hostGlobal<TempoUiApi>("tempo");
export const ipcRenderer = hostGlobal<IpcRendererApi>("ipcRenderer");
