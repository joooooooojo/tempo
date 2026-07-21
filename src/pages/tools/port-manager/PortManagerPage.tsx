import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  CircleStop,
  Loader2,
  Network,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
} from "lucide-react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { DataTable } from "@/components/ui/data-table";
import { TagList } from "@/components/ui/tag";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { PortRecord } from "@/types";

type ViewScope = "listening" | "all";
type ProtocolFilter = "all" | PortRecord["protocol"];
type LoadMode = "initial" | "manual" | "auto";

const AUTO_REFRESH_MS = 5_000;
const MIN_MANUAL_REFRESH_FEEDBACK_MS = 350;
const PAGE_SIZE = 50;
const SCOPE_ITEMS = [
  { value: "listening", label: "监听端口" },
  { value: "all", label: "全部连接" },
] as const;
const PROTOCOL_ITEMS = [
  { value: "all", label: "全部协议" },
  { value: "TCP", label: "TCP" },
  { value: "UDP", label: "UDP" },
] as const;

const STATE_LABELS: Record<string, string> = {
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

function isListening(record: PortRecord) {
  return record.protocol === "UDP" || record.state === "LISTEN";
}

function recordKey(record: PortRecord, index: number) {
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

export function PortManagerPage() {
  const navigate = useNavigate();
  const requestId = useRef(0);
  const inFlightScopes = useRef(new Set<ViewScope>());
  const manualRefreshStartedAt = useRef(new Map<ViewScope, number>());
  const snapshotSignature = useRef("");
  const hasRecords = useRef(false);
  const [records, setRecords] = useState<PortRecord[]>([]);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<ViewScope>("listening");
  const [protocol, setProtocol] = useState<ProtocolFilter>("all");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [page, setPage] = useState(0);
  const [pendingTermination, setPendingTermination] = useState<PortRecord | null>(null);
  const [terminating, setTerminating] = useState(false);

  const load = useCallback(async (mode: LoadMode = "initial") => {
    const requestedScope = scope;
    if (mode === "manual") {
      manualRefreshStartedAt.current.set(requestedScope, performance.now());
      setRefreshing(true);
    }
    if (inFlightScopes.current.has(requestedScope)) return;

    inFlightScopes.current.add(requestedScope);
    const currentRequest = ++requestId.current;
    if (mode === "initial") {
      if (!hasRecords.current) setLoading(true);
      else setRefreshing(true);
    }

    try {
      const next = await api.getPortRecords(requestedScope === "all");
      if (currentRequest !== requestId.current) return;
      const nextSignature = `${requestedScope}:${JSON.stringify(next)}`;
      const hasManualRefresh = manualRefreshStartedAt.current.has(requestedScope);
      if (nextSignature !== snapshotSignature.current) {
        snapshotSignature.current = nextSignature;
        if (hasRecords.current) startTransition(() => setRecords(next));
        else setRecords(next);
        hasRecords.current = true;
        setLastUpdated(new Date());
      } else if (mode !== "auto" || hasManualRefresh) {
        setLastUpdated(new Date());
      }
      setError(null);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      const message = loadError instanceof Error ? loadError.message : String(loadError);
      setError(message);
      if (mode === "manual" || manualRefreshStartedAt.current.has(requestedScope)) {
        toast.error(message);
      }
    } finally {
      inFlightScopes.current.delete(requestedScope);
      if (currentRequest === requestId.current) {
        setLoading(false);
        const manualStartedAt = manualRefreshStartedAt.current.get(requestedScope);
        if (manualStartedAt !== undefined) {
          const remaining = MIN_MANUAL_REFRESH_FEEDBACK_MS - (performance.now() - manualStartedAt);
          if (remaining > 0) {
            await new Promise((resolve) => window.setTimeout(resolve, remaining));
          }
          if (currentRequest === requestId.current) {
            manualRefreshStartedAt.current.delete(requestedScope);
            setRefreshing(false);
          }
        } else if (mode === "initial") {
          setRefreshing(false);
        }
      } else {
        manualRefreshStartedAt.current.delete(requestedScope);
      }
    }
  }, [scope]);

  useEffect(() => {
    let secondFrame = 0;
    let timer = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => {
        timer = window.setTimeout(() => void load(), 0);
      });
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      window.cancelAnimationFrame(secondFrame);
      window.clearTimeout(timer);
      requestId.current += 1;
    };
  }, [load]);

  useEffect(() => {
    if (!autoRefresh) return;
    const timer = window.setInterval(() => void load("auto"), AUTO_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [autoRefresh, load]);

  const filteredRecords = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return records.filter((record) => {
      if (scope === "listening" && !isListening(record)) return false;
      if (protocol !== "all" && record.protocol !== protocol) return false;
      if (!normalizedQuery) return true;

      return [
        record.localAddress,
        record.localPort,
        record.pid,
        record.processName,
        record.processPath,
        record.state,
      ]
        .filter((value) => value !== null && value !== undefined)
        .some((value) => String(value).toLocaleLowerCase().includes(normalizedQuery));
    });
  }, [protocol, query, records, scope]);

  useEffect(() => {
    setPage(0);
  }, [protocol, query, scope]);

  const totalPages = Math.max(1, Math.ceil(filteredRecords.length / PAGE_SIZE));
  const currentPage = Math.min(page, totalPages - 1);
  const visibleRecords = useMemo(
    () => filteredRecords.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE),
    [currentPage, filteredRecords]
  );

  const processCount = useMemo(
    () => new Set(filteredRecords.flatMap((record) => (record.pid ? [record.pid] : []))).size,
    [filteredRecords]
  );
  const listeningCount = useMemo(() => records.filter(isListening).length, [records]);

  const terminateProcess = async () => {
    const record = pendingTermination;
    if (!record?.pid || record.processStartedAt === null || record.processStartedAt === undefined) {
      return;
    }

    setTerminating(true);
    try {
      await api.terminatePortProcess({
        protocol: record.protocol,
        localAddress: record.localAddress,
        localPort: record.localPort,
        pid: record.pid,
        processStartedAt: record.processStartedAt,
      });
      toast.success(`已结束 ${record.processName}`);
      setPendingTermination(null);
      await new Promise((resolve) => window.setTimeout(resolve, 300));
      await load("auto");
    } catch (terminateError) {
      toast.error(terminateError instanceof Error ? terminateError.message : String(terminateError));
    } finally {
      setTerminating(false);
    }
  };

  const emptyContent = error ? (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Network />
        </EmptyMedia>
        <EmptyTitle>无法读取端口信息</EmptyTitle>
        <EmptyDescription>{error}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" onClick={() => void load()}>
          <RefreshCw data-icon="inline-start" />
          重试
        </Button>
      </EmptyContent>
    </Empty>
  ) : (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Network />
        </EmptyMedia>
        <EmptyTitle>没有匹配的端口</EmptyTitle>
        <EmptyDescription>调整筛选条件后再查看。</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 flex-col gap-3 border-b border-border/60 px-4 py-3">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => navigate("/tools")}
            aria-label="返回小工具"
            title="返回小工具"
          >
            <ArrowLeft />
          </Button>
          <div className="min-w-0 flex-1">
            <h1 className="text-[15px] font-semibold">端口管理器</h1>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {listeningCount} 个监听端口 · {processCount} 个进程
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Label htmlFor="port-manager-auto-refresh" className="text-[12px] text-muted-foreground">
              自动刷新
            </Label>
            <Switch
              id="port-manager-auto-refresh"
              checked={autoRefresh}
              onCheckedChange={setAutoRefresh}
              aria-label="自动刷新"
            />
          </div>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => void load("manual")}
            disabled={refreshing}
            aria-label="刷新端口列表"
            title="刷新"
          >
            <RefreshCw className={cn(refreshing && "animate-spin")} />
          </Button>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索端口、PID 或进程"
            aria-label="搜索端口、PID 或进程"
            className="min-w-52 flex-1 sm:max-w-sm"
          />
          <Select
            items={SCOPE_ITEMS}
            value={scope}
            onValueChange={(value) => value && setScope(value as ViewScope)}
          >
            <SelectTrigger className="w-28" aria-label="端口范围">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {SCOPE_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <Select
            items={PROTOCOL_ITEMS}
            value={protocol}
            onValueChange={(value) => value && setProtocol(value as ProtocolFilter)}
          >
            <SelectTrigger className="w-28" aria-label="网络协议">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {PROTOCOL_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>
      </header>

      <main className="flex min-h-0 flex-1 p-3">
        <DataTable
          loading={loading}
          loadingContent={
            <div className="flex w-full max-w-xl flex-col gap-2">
              {Array.from({ length: 6 }, (_, index) => (
                <Skeleton key={index} className="h-10 w-full" />
              ))}
            </div>
          }
          empty={!loading && filteredRecords.length === 0}
          emptyContent={emptyContent}
          scrollAreaLabel="本机端口列表"
          verticalScrollbarInsetTop="2.5rem"
          footer={
            <div className="flex items-center justify-between gap-3 border-t border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
              <span>{filteredRecords.length} 条记录</span>
              <div className="flex items-center gap-1">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={() => setPage((value) => Math.max(0, value - 1))}
                  disabled={currentPage === 0}
                  aria-label="上一页"
                  title="上一页"
                >
                  <ChevronLeft />
                </Button>
                <span className="min-w-14 text-center tabular-nums">
                  {currentPage + 1} / {totalPages}
                </span>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={() => setPage((value) => Math.min(totalPages - 1, value + 1))}
                  disabled={currentPage >= totalPages - 1}
                  aria-label="下一页"
                  title="下一页"
                >
                  <ChevronRight />
                </Button>
              </div>
              <span>
                {lastUpdated
                  ? `更新于 ${lastUpdated.toLocaleTimeString("zh-CN", { hour12: false })}`
                  : "尚未更新"}
              </span>
            </div>
          }
        >
          <Table>
            <TableHeader className="sticky top-0 z-30 bg-background">
              <TableRow>
                <TableHead className="pl-4">端口</TableHead>
                <TableHead>协议 / 状态</TableHead>
                <TableHead>进程</TableHead>
                <TableHead>PID</TableHead>
                <TableHead className="w-[24%] max-w-80">程序路径</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visibleRecords.map((record, index) => (
                <TableRow key={recordKey(record, currentPage * PAGE_SIZE + index)}>
                  <TableCell className="pl-4">
                    <div className="font-mono text-[14px] font-semibold tabular-nums">
                      {record.localPort}
                    </div>
                    <div className="font-mono text-[10px] text-muted-foreground">
                      {record.localAddress}
                    </div>
                  </TableCell>
                  <TableCell>
                    <TagList
                      items={[record.protocol, STATE_LABELS[record.state] ?? record.state]}
                      size="sm"
                    />
                  </TableCell>
                  <TableCell>
                    <div
                      className="text-[13px] font-medium"
                      title={record.processName}
                    >
                      {record.processName}
                    </div>
                    <div className="mt-0.5 text-[10px] text-muted-foreground">
                      {record.canTerminate ? "可结束" : record.protectedReason ?? "受保护"}
                    </div>
                  </TableCell>
                  <TableCell className="font-mono text-[12px] tabular-nums text-muted-foreground">
                    {record.pid ?? "-"}
                  </TableCell>
                  <TableCell className="w-[24%] max-w-80">
                    <div
                      className="max-w-full truncate text-[11px] text-muted-foreground"
                      title={record.processPath ?? undefined}
                    >
                      {record.processPath ?? "路径不可用"}
                    </div>
                  </TableCell>
                  <TableCell className="pr-4 text-right">
                    {record.canTerminate ? (
                      <Button
                        variant="destructive"
                        size="icon-sm"
                        onClick={() => setPendingTermination(record)}
                        aria-label={`结束 ${record.processName}`}
                        title="结束进程"
                      >
                        <CircleStop />
                      </Button>
                    ) : (
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        disabled
                        aria-label={record.protectedReason ?? "该进程不可结束"}
                        title={record.protectedReason ?? "该进程不可结束"}
                      >
                        <ShieldCheck />
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </DataTable>
      </main>

      <AlertDialog
        open={pendingTermination !== null}
        onOpenChange={(open) => {
          if (!open && !terminating) setPendingTermination(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogMedia>
              <TriangleAlert />
            </AlertDialogMedia>
            <AlertDialogTitle>结束 {pendingTermination?.processName}？</AlertDialogTitle>
            <AlertDialogDescription>
              PID {pendingTermination?.pid} 正在占用 {pendingTermination?.protocol} 端口{" "}
              {pendingTermination?.localPort}。这会终止整个进程并释放它占用的全部端口，未保存的内容可能丢失。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={terminating}>取消</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={terminating}
              onClick={() => void terminateProcess()}
            >
              {terminating ? (
                <Loader2 className="animate-spin" data-icon="inline-start" />
              ) : (
                <CircleStop data-icon="inline-start" />
              )}
              {terminating ? "正在结束" : "结束进程"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
