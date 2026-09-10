export function getHostGlobal<T>(name: string): T {
  const value = (globalThis as Record<string, unknown>)[name];
  if (value === undefined || value === null) {
    throw new Error(
      `Tempo SDK could not connect: host global \"${name}\" is unavailable`,
    );
  }
  return value as T;
}
