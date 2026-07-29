import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/api";
import type { PluginRuntimeStatus, RuntimeInstallProgress } from "@/types";
import { Row, Section } from "@/builtin-plugins/settings/pages/shared";

const RUNTIME_PROGRESS_EVENT = "plugin-runtime-install-progress";

function progressLabel(progress: RuntimeInstallProgress | null | undefined) {
  if (typeof progress?.percent === "number") return `${progress.percent}%`;
  if (progress?.phase === "extracting") return "解压中…";
  if (progress?.phase === "verifying") return "校验中…";
  return "下载中…";
}

export function PluginRuntimeSection() {
  const [runtime, setRuntime] = useState<PluginRuntimeStatus | null>(null);
  const [progress, setProgress] = useState<RuntimeInstallProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const toastDoneRef = useRef(false);

  const refresh = useCallback(async () => {
    const next = await api.getPluginRuntimeStatus();
    setRuntime(next);
    if (next.progress) {
      setProgress(next.progress);
    } else if (!next.installing) {
      setProgress(null);
    }
    return next;
  }, []);

  useEffect(() => {
    refresh().catch(console.error);
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listen<RuntimeInstallProgress>(RUNTIME_PROGRESS_EVENT, (event) => {
      if (cancelled) return;
      const next = event.payload;
      setProgress(next);
      setRuntime((prev) =>
        prev
          ? {
              ...prev,
              installing: next.phase !== "failed" && next.phase !== "done",
              message: next.message,
              progress: next,
            }
          : prev
      );
      if (next.phase === "done") {
        if (!toastDoneRef.current) {
          toastDoneRef.current = true;
          toast.success("插件运行时已安装");
        }
        void refresh();
      } else if (next.phase === "failed") {
        toast.error(next.message || "插件运行时安装失败");
        void refresh();
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refresh]);

  useEffect(() => {
    if (!runtime?.installing) return;
    toastDoneRef.current = false;
    const timer = window.setInterval(() => {
      void refresh().then((next) => {
        if (next.installed && !toastDoneRef.current) {
          toastDoneRef.current = true;
          toast.success("插件运行时已安装");
        }
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [runtime?.installing, refresh]);

  const installing =
    Boolean(runtime?.installing) ||
    Boolean(progress && progress.phase !== "failed" && progress.phase !== "done");

  const installRuntime = async () => {
    setBusy(true);
    toastDoneRef.current = false;
    try {
      const next = await api.installPluginRuntime();
      setRuntime(next);
      if (next.progress) setProgress(next.progress);
      if (next.installed) {
        toast.success("插件运行时已安装");
      } else if (!next.installing && next.progress?.phase === "failed") {
        toast.error(next.progress.message || next.message);
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const uninstallRuntime = async () => {
    if (!confirm("卸载后，含 main 的第三方插件将无法激活。确定继续？")) return;
    setBusy(true);
    try {
      const next = await api.uninstallPluginRuntime();
      setRuntime(next);
      setProgress(null);
      toast.success("已卸载插件运行时");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const desc = runtime?.installed
    ? runtime.version
      ? `已安装 v${runtime.version}`
      : "已安装"
    : undefined;

  return (
    <Section title="插件运行时">
      <Card>
        <Row label="插件运行时（Node）" desc={desc}>
          {runtime?.installed ? (
            <Button
              variant="outline"
              size="sm"
              disabled={busy || installing}
              onClick={() => void uninstallRuntime()}
            >
              卸载
            </Button>
          ) : (
            <Button size="sm" disabled={busy || installing} onClick={() => void installRuntime()}>
              {installing ? "安装中…" : "安装"}
            </Button>
          )}
        </Row>

        {installing || progress?.phase === "failed" ? (
          <div className="space-y-2 border-t border-border/50 px-4 py-3">
            {installing ? (
              <div className="space-y-1.5">
                <Progress value={progress?.percent ?? null} className="w-full" />
                <p className="text-[11px] text-muted-foreground">{progressLabel(progress)}</p>
              </div>
            ) : null}
            {progress?.phase === "failed" ? (
              <p className="text-[12px] text-destructive">{progress.message}</p>
            ) : null}
          </div>
        ) : null}
      </Card>
    </Section>
  );
}
