export type StorageGetFn = (key: string) => Promise<unknown>;
export type StorageSetFn = (key: string, value: unknown) => Promise<void>;
export type StorageDeleteFn = (key: string) => Promise<void>;
export type StorageListFn = () => Promise<string[]>;

export interface StorageAdapter {
  get: StorageGetFn;
  set: StorageSetFn;
  delete: StorageDeleteFn;
  list: StorageListFn;
}

export interface PluginStorageApi {
  get<T = unknown>(key: string): Promise<T | null>;
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  list(): Promise<string[]>;
  /** Read-modify-write helper for object values. */
  update<T extends Record<string, unknown>>(
    key: string,
    updater: (current: T | null) => T | Promise<T>
  ): Promise<T>;
}

export function createStorageApi(adapter: StorageAdapter): PluginStorageApi {
  return {
    async get<T = unknown>(key: string): Promise<T | null> {
      const value = await adapter.get(key);
      return (value ?? null) as T | null;
    },
    set(key, value) {
      return adapter.set(key, value);
    },
    delete(key) {
      return adapter.delete(key);
    },
    list() {
      return adapter.list();
    },
    async update<T extends Record<string, unknown>>(
      key: string,
      updater: (current: T | null) => T | Promise<T>
    ): Promise<T> {
      const current = (await adapter.get(key)) as T | null;
      const next = await updater(current);
      await adapter.set(key, next);
      return next;
    },
  };
}
