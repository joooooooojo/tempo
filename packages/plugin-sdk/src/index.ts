/**
 * `@tempo/plugin-sdk` — official Tempo plugin SDK.
 *
 * Import everything from this package; APIs are distinguished by name:
 * - Runtime: `definePlugin`, `wrapRuntimeContext`
 * - UI: `createPluginClient`, `createPluginClientSync`
 */

export * from "./constants.js";
export * from "./errors.js";
export * from "./types.js";
export * from "./storage.js";
export * from "./settings.js";
export * from "./host.js";
export * from "./ipc/structured-clone.js";
export {
  definePlugin,
  wrapRuntimeContext,
  type RawExtensionContext,
  type RuntimeCommandsApi,
  type RuntimeIpcApi,
  type RuntimeTempo,
  type PluginDefinition,
  type PluginModule,
  type IpcInvokeHandler,
} from "./runtime/index.js";
export {
  createPluginClient,
  createPluginClientSync,
  type TempoPluginUiBridge,
  type UiIpcApi,
  type UiTempo,
} from "./ui/index.js";
