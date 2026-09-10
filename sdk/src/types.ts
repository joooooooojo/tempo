export type TempoTheme = "light" | "dark" | "system";
export type TempoJsonPrimitive = string | number | boolean | null;
export type TempoJsonValue = TempoJsonPrimitive | TempoJsonObject | TempoJsonValue[];

export interface TempoJsonObject {
  [key: string]: TempoJsonValue;
}

export type TempoIpcPrimitive = string | number | boolean | bigint | null | undefined;
export type TempoIpcTypedArray =
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
export type TempoIpcValue =
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

export interface TempoIpcObject {
  [key: string]: TempoIpcValue;
}

export type TempoTextActionInput = {
  kind: "text";
  text: string;
};

export type TempoImageActionInput = {
  kind: "image";
  entryId: number;
  imageUrl: string;
  filePath?: string;
  width?: number | null;
  height?: number | null;
};

export type TempoFileActionInput = {
  kind: "file";
  entryId: number;
  paths: string[];
};

export type TempoActionInput =
  | TempoTextActionInput
  | TempoImageActionInput
  | TempoFileActionInput;

export type TempoActionInvocation = {
  actionId: string;
  query: string;
  input: TempoActionInput;
};

export type TempoContextParams = TempoActionInvocation | TempoJsonObject | null;
export type TempoSessionData = TempoJsonObject | null;
export type TempoSettingValue = string | boolean | string[];
export type TempoSettings = Record<string, TempoSettingValue>;
export type TempoWindowDimension = number | `${number}%`;
export type TempoWindowPosition = number | `${number}%` | "center";

export interface TempoWindowRect {
  width?: TempoWindowDimension;
  height?: TempoWindowDimension;
  x?: TempoWindowPosition;
  y?: TempoWindowPosition;
}

export interface TempoNotificationOptions {
  title?: string;
  body?: string;
}

export interface TempoClipboardChangedPayload {
  schemaVersion: 1;
  at: string;
}

export interface TempoHostEventMap {
  "clipboard.changed": TempoClipboardChangedPayload;
}

export type TempoHostEventName = keyof TempoHostEventMap & string;
export type TempoHostEventHandler<TEvent extends TempoHostEventName> = (
  payload: TempoHostEventMap[TEvent],
) => void;

export interface TempoEventsApi {
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

export interface TempoStorageApi {
  get<TValue extends TempoJsonValue = TempoJsonValue>(key: string): Promise<TValue | null>;
  set(key: string, value: TempoJsonValue): Promise<void>;
  delete(key: string): Promise<void>;
  list(): Promise<string[]>;
}

export type TempoFileType = "file" | "directory" | "other";

export interface TempoFileStat {
  path: string;
  type: TempoFileType;
  size: number | null;
  modifiedAt: string | null;
}

export interface TempoFileEntry extends TempoFileStat {
  name: string;
}

export interface TempoFileOperationOptions {
  recursive?: boolean;
}

export interface TempoFilesApi {
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

export interface TempoSettingsApi {
  getAll(): Promise<TempoSettings>;
  get<TValue extends TempoSettingValue = TempoSettingValue>(
    id: string,
    fallback?: TValue,
  ): Promise<TValue | undefined>;
  subscribe(handler: (values: TempoSettings) => void): () => void;
}

export interface TempoNotifyApi {
  show(options?: TempoNotificationOptions): Promise<void>;
}

export interface TempoAppApi {
  open(appId: string, params?: TempoJsonObject): Promise<void>;
}

export interface TempoExternalApi {
  open(url: string): Promise<void>;
}

export interface TempoUiContext {
  apiVersion: string;
  theme: TempoTheme;
  params: TempoContextParams;
  session: TempoSessionData;
}

export interface TempoUiThemeApi {
  get(): Promise<TempoTheme>;
  subscribe(handler: (theme: TempoTheme) => void): Promise<() => void>;
}

export interface TempoUiMainPanelApi {
  hide(): Promise<void>;
  back(): Promise<void>;
  setSize(height: number): Promise<void>;
}

export interface TempoWindowApi {
  setRect(rect: TempoWindowRect): Promise<void>;
  close(): Promise<void>;
}

export interface TempoSessionApi {
  push(payload: TempoJsonObject): Promise<void>;
}

export interface TempoUiApi {
  readonly context: TempoUiContext | null;
  ready(): Promise<TempoUiContext>;
  readonly events: TempoEventsApi;
  readonly storage: TempoStorageApi;
  readonly files: TempoFilesApi;
  readonly settings: TempoSettingsApi;
  readonly notify: TempoNotifyApi;
  readonly theme: TempoUiThemeApi;
  readonly mainPanel: TempoUiMainPanelApi;
  readonly window: TempoWindowApi;
  readonly app: TempoAppApi;
  readonly external: TempoExternalApi;
  readonly session: TempoSessionApi;
}

export interface IpcRendererEvent {
  readonly sender: "runtime";
}

export interface IpcRendererApi {
  invoke<
    TResult extends TempoIpcValue = TempoIpcValue,
    TArgs extends readonly TempoIpcValue[] = TempoIpcValue[],
  >(channel: string, ...args: TArgs): Promise<TResult>;
  send<TArgs extends readonly TempoIpcValue[]>(channel: string, ...args: TArgs): void;
  on<TArgs extends readonly TempoIpcValue[]>(
    channel: string,
    listener: (event: IpcRendererEvent, ...args: TArgs) => void,
  ): () => void;
}

export type TempoRuntimeResult = TempoJsonValue | void;
export type TempoLifecycleHook = () => void | Promise<void>;
export type TempoLifecycleRegistrar = (hook: TempoLifecycleHook) => void;

export interface IpcMainEventSender {
  send<TArgs extends readonly TempoIpcValue[]>(channel: string, ...args: TArgs): void;
}

export interface IpcMainEvent {
  readonly sender: IpcMainEventSender;
}

export interface IpcMainApi {
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

export interface TempoRuntimePaths {
  readonly data: string;
}

export interface TempoRuntimeInfo {
  readonly engine: "deno";
  readonly version: string;
  readonly nodeCompatVersion: string;
}

export interface TempoCommandsApi {
  register<TParams = TempoActionInvocation>(
    id: string,
    handler: (
      params: TParams,
      signal: AbortSignal,
    ) => TempoRuntimeResult | Promise<TempoRuntimeResult>,
  ): void;
}

export interface TempoMcpToolsApi {
  register<TParams = TempoJsonObject>(
    name: string,
    handler: (
      params: TParams,
      signal: AbortSignal,
    ) => TempoRuntimeResult | Promise<TempoRuntimeResult>,
  ): void;
}

export interface TempoRuntimeThemeApi {
  get(): Promise<TempoTheme>;
}

export interface TempoRuntimeMainPanelApi {
  hide(): Promise<void>;
}

export interface TempoRuntimeApi {
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

export interface PluginIpcContract {
  invokes?: object;
  messages?: object;
}

export type AnyPluginIpcContract = {
  invokes: Record<string, (...args: any[]) => any>;
  messages: Record<string, readonly any[]>;
};

type InvokeMap<TContract extends PluginIpcContract> =
  TContract extends { invokes: infer TInvokes }
    ? TInvokes
    : AnyPluginIpcContract["invokes"];

type MessageMap<TContract extends PluginIpcContract> =
  TContract extends { messages: infer TMessages }
    ? TMessages
    : AnyPluginIpcContract["messages"];

type InvokeChannel<TContract extends PluginIpcContract> =
  keyof InvokeMap<TContract> & string;

type MessageChannel<TContract extends PluginIpcContract> =
  keyof MessageMap<TContract> & string;

type InvokeArgs<
  TContract extends PluginIpcContract,
  TChannel extends InvokeChannel<TContract>,
> = InvokeMap<TContract>[TChannel] extends (...args: infer TArgs) => any
  ? TArgs
  : never;

type InvokeResult<
  TContract extends PluginIpcContract,
  TChannel extends InvokeChannel<TContract>,
> = InvokeMap<TContract>[TChannel] extends (...args: any[]) => infer TResult
  ? Awaited<TResult>
  : never;

type MessageArgs<
  TContract extends PluginIpcContract,
  TChannel extends MessageChannel<TContract>,
> = MessageMap<TContract>[TChannel] extends readonly any[]
  ? MessageMap<TContract>[TChannel]
  : never;

export interface TempoUiIpc<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> {
  invoke<TChannel extends InvokeChannel<TContract>>(
    channel: TChannel,
    ...args: InvokeArgs<TContract, TChannel>
  ): Promise<InvokeResult<TContract, TChannel>>;
  send<TChannel extends MessageChannel<TContract>>(
    channel: TChannel,
    ...args: MessageArgs<TContract, TChannel>
  ): void;
  on<TChannel extends MessageChannel<TContract>>(
    channel: TChannel,
    listener: (
      event: IpcRendererEvent,
      ...args: MessageArgs<TContract, TChannel>
    ) => void,
  ): () => void;
}

export interface TempoRuntimeIpcSender<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> {
  send<TChannel extends MessageChannel<TContract>>(
    channel: TChannel,
    ...args: MessageArgs<TContract, TChannel>
  ): void;
}

export interface TempoRuntimeIpcEvent<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> {
  readonly sender: TempoRuntimeIpcSender<TContract>;
}

export interface TempoRuntimeIpc<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> {
  handle<TChannel extends InvokeChannel<TContract>>(
    channel: TChannel,
    handler: (
      event: TempoRuntimeIpcEvent<TContract>,
      ...args: InvokeArgs<TContract, TChannel>
    ) =>
      | InvokeResult<TContract, TChannel>
      | Promise<InvokeResult<TContract, TChannel>>,
  ): void;
  on<TChannel extends MessageChannel<TContract>>(
    channel: TChannel,
    listener: (
      event: TempoRuntimeIpcEvent<TContract>,
      ...args: MessageArgs<TContract, TChannel>
    ) => void,
  ): () => void;
  send<TChannel extends MessageChannel<TContract>>(
    channel: TChannel,
    ...args: MessageArgs<TContract, TChannel>
  ): void;
}

export interface TempoUiClient<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> extends Omit<TempoUiApi, "app" | "context" | "ready"> {
  readonly context: TempoUiContext;
  readonly apps: TempoAppApi;
  readonly ipc: TempoUiIpc<TContract>;
}

export type TempoDisposer = () => void | Promise<void>;

export interface TempoRuntimeContext<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> extends Omit<TempoRuntimeApi, "app"> {
  readonly apps: TempoAppApi;
  readonly ipc: TempoRuntimeIpc<TContract>;
  onDispose(disposer: TempoDisposer): void;
}

export type TempoRuntimeSetup<
  TContract extends PluginIpcContract = AnyPluginIpcContract,
> = (
  context: TempoRuntimeContext<TContract>,
) => void | TempoDisposer | Promise<void | TempoDisposer>;
