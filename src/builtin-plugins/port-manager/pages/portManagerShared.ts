import type { PortRecord } from "@/types";

export type ViewScope = "listening" | "all";
export type ProtocolFilter = "all" | PortRecord["protocol"];
export type LoadMode = "initial" | "manual" | "auto";

export const AUTO_REFRESH_MS = 5_000;
export const MIN_MANUAL_REFRESH_FEEDBACK_MS = 350;
export const PAGE_SIZE = 50;
export const SCOPE_ITEMS = [
  { value: "listening", label: "监听端口" },
  { value: "all", label: "全部连接" },
] as const;
export const PROTOCOL_ITEMS = [
  { value: "all", label: "全部协议" },
  { value: "TCP", label: "TCP" },
  { value: "UDP", label: "UDP" },
] as const;

export const STATE_LABELS: Record<string, string> = {
  LISTEN: "监听",
  ESTABLISHED: "已连接",
  SYN_SENT: "等待响应",
  SYN_RCVD: "正在握手",
  FIN_WAIT_1: "正在关闭",
  FIN_WAIT_2: "等待关闭",
  CLOSE_WAIT: "等待本机关闭",
  CLOSING: "正在关闭",
  LAST_ACK: "等待确认",
  TIME_WAIT: "等待释放",
  CLOSED: "已关闭",
  DELETE_TCB: "正在删除",
  BOUND: "已绑定",
  __UNKNOWN: "未知",
};

export function isListening(record: PortRecord) {
  return record.protocol === "UDP" || record.state === "LISTEN";
}

export function recordKey(record: PortRecord, index: number) {
  return [
    record.protocol,
    record.localAddress,
    record.localPort,
    record.remoteAddress,
    record.remotePort,
    record.pid,
    index,
  ].join(":");
}
