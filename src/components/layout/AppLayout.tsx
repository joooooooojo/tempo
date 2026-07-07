import { NavLink, Outlet, useLocation } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BarChart3, Clock, Info, Minus, Settings, Square, X } from "lucide-react";
import { useState } from "react";
import type { MouseEvent } from "react";
import { cn } from "@/lib/utils";

const navItems = [
  { to: "/", label: "概览", icon: Clock },
  { to: "/reports", label: "报表", icon: BarChart3 },
  { to: "/settings", label: "设置", icon: Settings },
  { to: "/about", label: "关于", icon: Info },
];

export function AppLayout() {
  const location = useLocation();

  return (
    <div className="app-shell flex h-screen flex-col">
      <WindowTitleBar />

      <div className="relative z-10 flex min-h-0 flex-1">
        <aside className="flex w-[200px] shrink-0 flex-col border-r border-border/60 p-4 pt-5">
          <nav className="flex flex-1 flex-col gap-1">
            {navItems.map(({ to, label, icon: Icon }) => (
              <NavLink
                key={to}
                to={to}
                end={to === "/"}
                className={({ isActive }) =>
                  cn(
                    "nav-item group flex items-center gap-3 rounded-lg px-3 py-2.5 text-[13px] font-medium transition-all duration-200",
                    isActive
                      ? "bg-primary/15 text-primary shadow-sm"
                      : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                  )
                }
              >
                {({ isActive }) => (
                  <>
                    <span
                      className={cn(
                        "nav-icon flex h-7 w-7 items-center justify-center rounded-lg transition-all",
                        isActive
                          ? "bg-primary text-primary-foreground shadow-md shadow-primary/30"
                          : "bg-foreground/5 group-hover:bg-foreground/8"
                      )}
                    >
                      <Icon className="h-3.5 w-3.5" strokeWidth={2} />
                    </span>
                    {label}
                  </>
                )}
              </NavLink>
            ))}
          </nav>

          <div className="glass-subtle mt-auto rounded-lg p-3">
            <p className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              状态
            </p>
            <div className="mt-1.5 flex items-center gap-2">
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-[3px] bg-emerald-400 opacity-60" />
                <span className="relative inline-flex h-2 w-2 rounded-[3px] bg-emerald-400" />
              </span>
              <span className="text-[11px] text-muted-foreground">后台统计中</span>
            </div>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <main className="no-scrollbar flex-1 overflow-y-auto px-6 py-5">
            <div key={location.pathname} className="page-transition">
              <Outlet />
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}

function WindowTitleBar() {
  const appWindow = getCurrentWindow();
  const [hoveredControl, setHoveredControl] = useState<"minimize" | "close" | null>(null);

  const startDrag = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest("[data-no-drag]")) return;

    event.preventDefault();
    appWindow.startDragging().catch(console.error);
  };

  const resetControlState = (button?: HTMLButtonElement | null) => {
    setHoveredControl(null);
    button?.blur();
  };

  const minimizeWindow = async (button: HTMLButtonElement) => {
    resetControlState(button);
    await appWindow.minimize().catch(console.error);
  };

  const hideWindow = async (button: HTMLButtonElement) => {
    resetControlState(button);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await appWindow.hide().catch(console.error);
  };

  return (
    <div
      data-tauri-drag-region
      onMouseDown={startDrag}
      className="relative z-20 flex h-10 shrink-0 select-none items-center bg-transparent pl-4"
    >
      <div data-tauri-drag-region className="text-[13px] font-medium">
        时窗
      </div>
      <div data-tauri-drag-region className="h-full flex-1" />
      <div data-no-drag className="flex h-full">
        <button
          type="button"
          className={cn(
            "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors focus:outline-none",
            hoveredControl === "minimize" && "bg-foreground/5 text-foreground"
          )}
          aria-label="最小化"
          onPointerEnter={() => setHoveredControl("minimize")}
          onPointerLeave={() => setHoveredControl(null)}
          onPointerDown={(event) => resetControlState(event.currentTarget)}
          onClick={(event) => void minimizeWindow(event.currentTarget)}
        >
          <Minus className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          className="flex h-full w-11 items-center justify-center text-muted-foreground opacity-35 transition-colors focus:outline-none"
          aria-label="最大化"
          disabled
        >
          <Square className="h-3 w-3" />
        </button>
        <button
          type="button"
          className={cn(
            "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors focus:outline-none",
            hoveredControl === "close" && "bg-foreground/6 text-foreground"
          )}
          aria-label="关闭"
          onPointerEnter={() => setHoveredControl("close")}
          onPointerLeave={() => setHoveredControl(null)}
          onPointerDown={(event) => resetControlState(event.currentTarget)}
          onClick={(event) => void hideWindow(event.currentTarget)}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
