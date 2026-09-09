export function hostGlobal<T>(name: string): T {
  const value = (globalThis as Record<string, unknown>)[name];
  if (value === undefined || value === null) {
    throw new Error(`Tempo host global \"${name}\" is unavailable`);
  }
  return value as T;
}
