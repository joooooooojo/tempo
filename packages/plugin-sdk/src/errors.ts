export type RpcErrorCode =
  | "INVALID_REQUEST"
  | "PAYLOAD_TOO_LARGE"
  | "RESOURCE_EXHAUSTED"
  | "NOT_FOUND"
  | "FORBIDDEN"
  | "TIMEOUT"
  | "CANCELLED"
  | "ACTIVATION_FAILED"
  | "RUNTIME_UNAVAILABLE"
  | "COMMAND_FAILED"
  | "INTERNAL";

export interface RpcErrorShape {
  code: RpcErrorCode | string;
  message: string;
  data?: unknown;
}

/**
 * Throw from a Runtime command handler to return `COMMAND_FAILED` with your message/data.
 * Plain `Error` is also wrapped as `COMMAND_FAILED` by the bootstrap.
 */
export class PluginCommandError extends Error {
  readonly data?: unknown;

  constructor(message: string, data?: unknown) {
    super(message);
    this.name = "PluginCommandError";
    this.data = data;
  }
}

export class HostRpcError extends Error implements RpcErrorShape {
  readonly code: RpcErrorCode | string;
  readonly data?: unknown;

  constructor(error: RpcErrorShape) {
    super(error.message || "host call failed");
    this.name = "HostRpcError";
    this.code = error.code || "INTERNAL";
    this.data = error.data;
  }
}

export function toHostRpcError(error: unknown): HostRpcError {
  if (error instanceof HostRpcError) return error;
  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    if (typeof record.message === "string") {
      return new HostRpcError({
        code: typeof record.code === "string" ? record.code : "INTERNAL",
        message: record.message,
        data: record.data,
      });
    }
  }
  return new HostRpcError({
    code: "INTERNAL",
    message: error instanceof Error ? error.message : String(error),
  });
}
