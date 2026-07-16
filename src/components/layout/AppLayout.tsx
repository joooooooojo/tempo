import { NavLink, Outlet, useLocation } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  BarChart3,
  ClipboardList,
  Copy,
  ListTodo,
  Minus,
  PanelLeft,
  PanelLeftClose,
  Settings,
  Square,
  TextQuote,
  Timer,
  Wrench,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { NavSidebarProvider } from "@/components/layout/nav-sidebar";
import { api } from "@/lib/api";
import { cn, isMacTarget } from "@/lib/utils";

const macOS = isMacTarget;

const navItems = [
  { to: "/", label: "待办事项", icon: ListTodo },
  { to: "/pomodoro", label: "番茄时钟", icon: Timer },
  { to: "/reports", label: "屏幕使用时间", icon: BarChart3 },
  { to: "/clipboard", label: "剪贴板", icon: ClipboardList },
  { to: "/snippets", label: "快捷短语", icon: TextQuote },
  { to: "/tools", label: "小工具", icon: Wrench },
  { to: "/settings", label: "设置", icon: Settings },
];

function isToolsHub(pathname: string) {
  return pathname === "/tools";
}

function isToolDetail(pathname: string) {
  return pathname.startsWith("/tools/");
}

export function AppLayout() {
  const location = useLocation();
  const isTodoPage = location.pathname === "/";
  const toolsHub = isToolsHub(location.pathname);
  const toolDetail = isToolDetail(location.pathname);
  const isFullHeightPage =
    isTodoPage ||
    location.pathname === "/clipboard" ||
    toolsHub ||
    toolDetail;

  const [collapsed, setCollapsed] = useState(false);

  const navSidebarValue = useMemo(
    () => ({
      collapsed,
      setCollapsed,
      toggleCollapsed: () => setCollapsed((v) => !v),
      hidden: toolDetail,
    }),
    [collapsed, toolDetail]
  );

  const asideState = toolDetail ? "hidden" : collapsed ? "collapsed" : "expanded";

  return (
    <NavSidebarProvider value={navSidebarValue}>
      <div className="app-shell relative flex h-screen flex-col overflow-hidden">
        {macOS ? (
          <>
            <div data-tauri-drag-region className="mac-titlebar-inset h-8 shrink-0" aria-hidden="true" />
          </>
        ) : (
          <WindowTitleBar />
        )}

        <div className="relative z-10 flex min-h-0 flex-1">
          <aside
            className={cn("app-nav-aside flex shrink-0 flex-col overflow-hidden border-r border-border/60", {
              "app-nav-aside--expanded": asideState === "expanded",
              "app-nav-aside--collapsed": asideState === "collapsed",
              "app-nav-aside--hidden": asideState === "hidden",
            })}
            aria-hidden={asideState === "hidden"}
          >
            <div
              className={cn(
                "app-nav-aside-inner flex h-full flex-col pb-4 pt-1",
                collapsed ? "w-[3.75rem] items-center px-1.5" : "w-50 px-4"
              )}
            >
              <nav className={cn("flex flex-1 flex-col gap-1", collapsed && "w-full items-center")}>
                {navItems.map(({ to, label, icon: Icon }) => (
                  <NavLink
                    key={to}
                    to={to}
                    end={to === "/"}
                    title={collapsed ? label : undefined}
                    tabIndex={asideState === "hidden" ? -1 : undefined}
                    className={({ isActive }) =>
                      cn(
                        "nav-item group flex items-center rounded-lg text-[13px] font-medium transition-all duration-200",
                        collapsed ? "size-10 justify-center px-0" : "gap-3 px-3 py-2.5",
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
                            "nav-icon flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-all",
                            isActive
                              ? "bg-primary text-primary-foreground shadow-md shadow-primary/30"
                              : "bg-foreground/5 group-hover:bg-foreground/8"
                          )}
                        >
                          <Icon className="h-3.5 w-3.5" strokeWidth={2} />
                        </span>
                        <span
                          className={cn(
                            "truncate transition-[opacity,width,margin] duration-200",
                            collapsed ? "w-0 overflow-hidden opacity-0" : "opacity-100"
                          )}
                        >
                          {label}
                        </span>
                      </>
                    )}
                  </NavLink>
                ))}
              </nav>
              <div
                className={cn(
                  "mt-4 flex w-full items-center border-t border-border/50 pt-3",
                  collapsed ? "flex-col gap-2" : "justify-between gap-2"
                )}
              >
                <div
                  className={cn(
                    "text-[11px] font-medium text-muted-foreground/75 transition-opacity duration-200",
                    collapsed && "sr-only"
                  )}
                >
                  v{__APP_VERSION__}
                </div>
                <button
                  type="button"
                  className="inline-flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
                  aria-label={collapsed ? "展开菜单" : "收起菜单"}
                  onClick={() => setCollapsed((v) => !v)}
                >
                  {collapsed ? <PanelLeft className="size-4" /> : <PanelLeftClose className="size-4" />}
                </button>
              </div>
            </div>
          </aside>

          <div className="relative flex min-w-0 flex-1 flex-col">
            <main
              className={cn(
                "no-scrollbar flex-1 px-4 pb-4 pt-1",
                isFullHeightPage ? "flex min-h-0 flex-col overflow-hidden" : "overflow-y-auto"
              )}
            >
              <div
                className={cn("page-transition", isFullHeightPage && "flex min-h-0 flex-1 flex-col")}
              >
                <Outlet />
              </div>
            </main>
          </div>
        </div>
      </div>
    </NavSidebarProvider>
  );
}

function WindowTitleBar() {
  const appWindow = getCurrentWindow();
  const [hoveredControl, setHoveredControl] = useState<"minimize" | "maximize" | "close" | null>(null);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let disposed = false;
    const syncMaximized = async () => {
      const next = await appWindow.isMaximized().catch(() => false);
      if (!disposed) setMaximized(next);
    };

    void syncMaximized();
    const unlistenPromise = appWindow.onResized(() => {
      void syncMaximized();
    });

    return () => {
      disposed = true;
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [appWindow]);

  const resetControlState = (button?: HTMLButtonElement | null) => {
    setHoveredControl(null);
    button?.blur();
  };

  const minimizeWindow = async (button: HTMLButtonElement) => {
    resetControlState(button);
    await appWindow.minimize().catch(console.error);
  };

  const toggleMaximizeWindow = async (button: HTMLButtonElement) => {
    resetControlState(button);
    await appWindow.toggleMaximize().catch(console.error);
    const next = await appWindow.isMaximized().catch(() => false);
    setMaximized(next);
  };

  const hideWindow = async (button: HTMLButtonElement) => {
    resetControlState(button);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await api.hideToTray().catch(console.error);
  };

  return (
    <div
      data-tauri-drag-region
      className="window-titlebar relative z-20 flex h-10 shrink-0 select-none items-center pl-4"
      onDoubleClick={() => void appWindow.toggleMaximize().catch(console.error)}
    >
      <div data-tauri-drag-region className="text-[13px] font-medium text-foreground/82">
        Tempo
      </div>
      <div data-tauri-drag-region className="h-full flex-1" />
      <div data-no-drag className="flex h-full [-webkit-app-region:no-drag] [app-region:no-drag]">
        <button
          type="button"
          className={cn(
            "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors focus:outline-none !cursor-default",
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
          className={cn(
            "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors focus:outline-none !cursor-default",
            hoveredControl === "maximize" && "bg-foreground/5 text-foreground"
          )}
          aria-label={maximized ? "还原" : "最大化"}
          onPointerEnter={() => setHoveredControl("maximize")}
          onPointerLeave={() => setHoveredControl(null)}
          onPointerDown={(event) => resetControlState(event.currentTarget)}
          onClick={(event) => void toggleMaximizeWindow(event.currentTarget)}
        >
          {maximized ? <Copy className="h-3 w-3" /> : <Square className="h-3 w-3" />}
        </button>
        <button
          type="button"
          className={cn(
            "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors focus:outline-none !cursor-default",
            hoveredControl === "close" && "bg-rose-500/12 text-rose-600 dark:text-rose-300"
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
