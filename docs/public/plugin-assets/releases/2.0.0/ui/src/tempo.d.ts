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

  type TempoContextParams = TempoActionInvocation | TempoJsonObject | null;
  type TempoSessionData = TempoJsonObject | null;
  type TempoSettingValue = string | boolean | string[];
  type TempoSettings = Record<string, TempoSettingValue>;
  type TempoWindowDimension = number | `${number}%`;
  type TempoWindowPosition = number | `${number}%` | "center";

  interface TempoWindowRect {
    width?: TempoWindowDimension;
    height?: TempoWindowDimension;
    x?: TempoWindowPosition;
    y?: TempoWindowPosition;
  }

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

  interface Window {
    tempo: TempoUiApi;
    ipcRenderer: IpcRendererApi;
  }

  interface IpcRendererEvent {
    readonly sender: "runtime";
  }

  interface IpcRendererApi {
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

  interface TempoUiContext {
    apiVersion: string;
    theme: TempoTheme;
    params: TempoContextParams;
    session: TempoSessionData;
  }

  interface TempoUiApi {
    readonly context: TempoUiContext | null;
    ready(): Promise<TempoUiContext>;
    events: TempoEventsApi;
    storage: TempoStorageApi;
    settings: TempoSettingsApi;
    notify: TempoNotifyApi;
    theme: TempoUiThemeApi;
    mainPanel: TempoUiMainPanelApi;
    window: TempoWindowApi;
    app: TempoAppApi;
    external: TempoExternalApi;
    session: TempoSessionApi;
  }

  interface TempoStorageApi {
    get<TValue extends TempoJsonValue = TempoJsonValue>(key: string): Promise<TValue | null>;
    set(key: string, value: TempoJsonValue): Promise<void>;
    delete(key: string): Promise<void>;
    list(): Promise<string[]>;
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

  interface TempoUiThemeApi {
    get(): Promise<TempoTheme>;
    subscribe(handler: (theme: TempoTheme) => void): Promise<() => void>;
  }

  interface TempoUiMainPanelApi {
    hide(): Promise<void>;
    back(): Promise<void>;
    setSize(height: number): Promise<void>;
  }

  interface TempoWindowApi {
    setRect(rect: TempoWindowRect): Promise<void>;
    close(): Promise<void>;
  }

  interface TempoAppApi {
    open(appId: string, params?: TempoJsonObject): Promise<void>;
  }

  interface TempoExternalApi {
    open(url: string): Promise<void>;
  }

  interface TempoSessionApi {
    push(payload: TempoJsonObject): Promise<void>;
  }
}
