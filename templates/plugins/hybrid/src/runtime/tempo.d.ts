export {};

declare global {
  type TempoTheme = "light" | "dark" | "system";
  type TempoJsonPrimitive = string | number | boolean | null;
  type TempoJsonValue =
    | TempoJsonPrimitive
    | TempoJsonObject
    | TempoJsonValue[];

  interface TempoJsonObject {
    [key: string]: TempoJsonValue;
  }

  type TempoIpcPrimitive = string | number | boolean | bigint | null | undefined;
  type TempoIpcTypedArray =
    | Int8Array
    | Uint8Array
    | Uint8ClampedArray
    | Int16Array
    | Uint16Array
    | Int32Array
    | Uint32Array
    | Float32Array
    | Float64Array
    | BigInt64Array
    | BigUint64Array;
  type TempoIpcValue =
    | TempoIpcPrimitive
    | Date
    | Error
    | ArrayBuffer
    | DataView
    | TempoIpcTypedArray
    | TempoIpcObject
    | TempoIpcValue[]
    | Map<TempoIpcValue, TempoIpcValue>
    | Set<TempoIpcValue>;

  interface TempoIpcObject {
    [key: string]: TempoIpcValue;
  }

  type TempoTextActionInput = {
    kind: "text";
    text: string;
  }

  type TempoImageActionInput = {
    kind: "image";
    entryId: number;
    imageUrl: string;
    filePath?: string;
    width?: number | null;
    height?: number | null;
  }

  type TempoFileActionInput = {
    kind: "file";
    entryId: number;
    paths: string[];
  }

  type TempoActionInput =
    | TempoTextActionInput
    | TempoImageActionInput
    | TempoFileActionInput;

  type TempoActionInvocation = {
    actionId: string;
    query: string;
    input: TempoActionInput;
  }

  type TempoSettingValue = string | boolean | string[];
  type TempoSettings = Record<string, TempoSettingValue>;

  interface TempoNotificationOptions {
    title?: string;
    body?: string;
  }

  interface TempoClipboardChangedPayload {
    schemaVersion: 1;
    at: string;
  }

  interface TempoHostEventMap {
    "clipboard.changed": TempoClipboardChangedPayload;
  }

  type TempoHostEventName = keyof TempoHostEventMap & string;
  type TempoHostEventHandler<TEvent extends TempoHostEventName> = (
    payload: TempoHostEventMap[TEvent],
  ) => void;
  type TempoRuntimeResult = TempoJsonValue | void;

  var tempo: TempoRuntimeApi;
  var ipcMain: IpcMainApi;
  function onMounted(hook: TempoLifecycleHook): void;
  function onUnmounted(hook: TempoLifecycleHook): void;

  type TempoLifecycleHook = () => void | Promise<void>;

  interface IpcMainEventSender {
    send<TArgs extends readonly TempoIpcValue[]>(channel: string, ...args: TArgs): void;
  }

  interface IpcMainEvent {
    readonly sender: IpcMainEventSender;
  }

  interface IpcMainApi {
    handle<
      TArgs extends readonly TempoIpcValue[] = TempoIpcValue[],
      TResult extends TempoIpcValue | void = TempoIpcValue | void,
    >(
      channel: string,
      handler: (event: IpcMainEvent, ...args: TArgs) => TResult | Promise<TResult>,
    ): void;
    on<TArgs extends readonly TempoIpcValue[]>(
      channel: string,
      listener: (event: IpcMainEvent, ...args: TArgs) => void,
    ): () => void;
    send<TArgs extends readonly TempoIpcValue[]>(channel: string, ...args: TArgs): void;
  }

  interface TempoRuntimeApi {
    readonly pluginId: string;
    readonly paths: TempoRuntimePaths;
    readonly runtime: TempoRuntimeInfo;
    readonly commands: TempoCommandsApi;
    readonly mcpTools: TempoMcpToolsApi;
    readonly events: TempoEventsApi;
    readonly storage: TempoStorageApi;
    readonly files: TempoFilesApi;
    readonly settings: TempoSettingsApi;
    readonly notify: TempoNotifyApi;
    readonly theme: TempoRuntimeThemeApi;
    readonly mainPanel: TempoRuntimeMainPanelApi;
    readonly app: TempoAppApi;
    readonly external: TempoExternalApi;
  }

  interface TempoRuntimePaths {
    readonly data: string;
  }

  interface TempoRuntimeInfo {
    readonly engine: "deno";
    readonly version: string;
    readonly nodeCompatVersion: string;
  }

  interface TempoCommandsApi {
    register<TParams = TempoActionInvocation>(
      id: string,
      handler: (
        params: TParams,
        signal: AbortSignal,
      ) => TempoRuntimeResult | Promise<TempoRuntimeResult>,
    ): void;
  }

  interface TempoMcpToolsApi {
    register<TParams = TempoJsonObject>(
      name: string,
      handler: (
        params: TParams,
        signal: AbortSignal,
      ) => TempoRuntimeResult | Promise<TempoRuntimeResult>,
    ): void;
  }

  interface TempoEventsApi {
    on<TEvent extends TempoHostEventName>(
      event: TEvent,
      handler: TempoHostEventHandler<TEvent>,
    ): () => void;
    once<TEvent extends TempoHostEventName>(
      event: TEvent,
      handler: TempoHostEventHandler<TEvent>,
    ): () => void;
    off<TEvent extends TempoHostEventName>(
      event: TEvent,
      handler: TempoHostEventHandler<TEvent>,
    ): boolean;
    removeAllListeners(event?: TempoHostEventName): void;
    listenerCount(event: TempoHostEventName): number;
    eventNames(): TempoHostEventName[];
  }

  interface TempoStorageApi {
    get<TValue extends TempoJsonValue = TempoJsonValue>(key: string): Promise<TValue | null>;
    set(key: string, value: TempoJsonValue): Promise<void>;
    delete(key: string): Promise<void>;
    list(): Promise<string[]>;
  }

  type TempoFileType = "file" | "directory" | "other";

  interface TempoFileStat {
    path: string;
    type: TempoFileType;
    size: number | null;
    modifiedAt: string | null;
  }

  interface TempoFileEntry extends TempoFileStat {
    name: string;
  }

  interface TempoFileOperationOptions {
    recursive?: boolean;
  }

  interface TempoFilesApi {
    readText(path: string): Promise<string>;
    writeText(path: string, content: string): Promise<void>;
    readBytes(path: string): Promise<Uint8Array>;
    writeBytes(path: string, bytes: Uint8Array): Promise<void>;
    list(path?: string): Promise<TempoFileEntry[]>;
    stat(path: string): Promise<TempoFileStat | null>;
    mkdir(path: string, options?: TempoFileOperationOptions): Promise<void>;
    remove(path: string, options?: TempoFileOperationOptions): Promise<void>;
    rename(from: string, to: string): Promise<void>;
  }

  interface TempoSettingsApi {
    getAll(): Promise<TempoSettings>;
    get<TValue extends TempoSettingValue = TempoSettingValue>(
      id: string,
      fallback?: TValue,
    ): Promise<TValue | undefined>;
    subscribe(handler: (values: TempoSettings) => void): () => void;
  }

  interface TempoNotifyApi {
    show(options?: TempoNotificationOptions): Promise<void>;
  }

  interface TempoRuntimeThemeApi {
    get(): Promise<TempoTheme>;
  }

  interface TempoRuntimeMainPanelApi {
    hide(): Promise<void>;
  }

  interface TempoAppApi {
    open(appId: string, params?: TempoJsonObject): Promise<void>;
  }

  interface TempoExternalApi {
    open(url: string): Promise<void>;
  }
}
