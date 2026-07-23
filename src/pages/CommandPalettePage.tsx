import {
  useCallback,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { isBlurHideSuppressed } from "@/lib/blurHideGuard";
import {
  LoaderCircle,
  Pin,
  PinOff,
  ArrowLeft,
} from "lucide-react";
import { Toaster, toast } from "sonner";
import { AppIcon } from "@/components/AppIcon";
import { OnboardingDialog } from "@/components/OnboardingDialog";
import { ReminderDialog } from "@/components/ReminderDialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import {
  listVisibleQuickActions,
  quickActionUsageId,
} from "@/apps/actions/registry";
import { AppIconView } from "@/apps/icon";
import { BuiltinAppNavigationProvider } from "@/apps/navigation";
import { PluginAppHost } from "@/apps/PluginAppHost";
import { startPluginContributionSync } from "@/apps/plugins/syncContributions";
import { getApp as getBuiltinApp, listApps as listBuiltinApps, subscribeApps } from "@/apps/registry";
import {
  canPersistAppSession,
  clearPaletteSession,
  resolveRestorablePaletteSession,
  writePaletteSession,
} from "@/apps/session";
import {
  resolveOpenAppParams,
  type BuiltinApp,
  type OpenBuiltinAppOptions,
  type QuickAction,
} from "@/apps/types";
import { api } from "@/lib/api";
import { notifyUser } from "@/lib/notifications";
import { playNotificationSound } from "@/lib/sound";
import { applyTheme, subscribeThemeChanges } from "@/lib/theme";
import { appToastOptions } from "@/lib/toastOptions";
import { isMacTarget, isWindowsTarget, cn } from "@/lib/utils";
import type { LauncherApp, LauncherUsageItem, ReminderEvent } from "@/types";

const GRID_COLUMNS = 9;
const RECENT_COLLAPSED_COUNT = GRID_COLUMNS * 2;
const PINNED_COLLAPSED_COUNT = GRID_COLUMNS;
const SEARCH_COLLAPSED_COUNT = GRID_COLUMNS * 2;
const MAX_SEARCH_RESULTS = GRID_COLUMNS * 4;
/** Typing changes result height often; native window resize each time feels like input lag. */
const PALETTE_RESIZE_DEBOUNCE_MS = 100;
const SEARCH_WIDTH = 800;
/** Fallback only when content has not mounted yet; prefer measured scrollHeight. */
const SEARCH_FALLBACK_HEIGHT = 370;
const DEFAULT_APP_HEIGHT = 580;
const BUILTIN_USAGE_PREFIX = "builtin:";
const PLUGIN_USAGE_PREFIX = "plugin:";
/** Tool pages already have their own edge-to-edge chrome; skip host padding. */
const FLUSH_APP_IDS = new Set(["hosts", "translate", "port-manager"]);
/** Fill host via h-full/flex — do not wrap in ScrollArea (breaks height chain). */
const FILL_HEIGHT_APP_IDS = new Set([
  "hosts",
  "translate",
  "port-manager",
  "todo",
  "pomodoro",
]);

type PaletteMode = "search" | "app";

type PaletteSelection =
  | { key: string; kind: "app"; app: LauncherApp }
  | { key: string; kind: "builtin"; app: BuiltinApp }
  | { key: string; kind: "action"; action: QuickAction };

type RecentEntry =
  | {
      key: string;
      kind: "app";
      app: LauncherApp;
      last_used_at: string | null;
      use_count: number;
    }
  | {
      key: string;
      kind: "builtin";
      app: BuiltinApp;
      last_used_at: string | null;
      use_count: number;
    };

type OpenAppPayload = {
  appId: string;
  createSnippet?: boolean;
};

function builtinUsageId(appId: string) {
  return `${BUILTIN_USAGE_PREFIX}${appId}`;
}

function usageTimeMs(value: string | null | undefined): number {
  if (!value) return 0;
  const ms = Date.parse(value);
  return Number.isFinite(ms) ? ms : 0;
}

export function CommandPalettePage() {
  const [mode, setMode] = useState<PaletteMode>("search");
  const [activeAppId, setActiveAppId] = useState<string | null>(null);
  const [activeAppParams, setActiveAppParams] = useState<Record<string, unknown>>({});
  const [openCreateSnippet, setOpenCreateSnippet] = useState(false);
  const [initialTranslateText, setInitialTranslateText] = useState<string | undefined>();
  const [apps, setApps] = useState<LauncherApp[]>([]);
  const [usageItems, setUsageItems] = useState<LauncherUsageItem[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [recentExpanded, setRecentExpanded] = useState(false);
  const [pinnedExpanded, setPinnedExpanded] = useState(false);
  const [searchExpanded, setSearchExpanded] = useState(false);
  const [openRevision, setOpenRevision] = useState(0);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [reminder, setReminder] = useState<ReminderEvent | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const pendingRef = useRef<string | null>(null);
  const modeRef = useRef<PaletteMode>("search");
  const activeAppIdRef = useRef<string | null>(null);
  /** After leaving a plugin, size once to measured search content (skip placeholder 370). */
  const needsSearchSizeRef = useRef(false);
  const isTauri = isTauriRuntime();
  const [appsRevision, setAppsRevision] = useState(0);
  const builtinApps = useMemo(() => listBuiltinApps(), [appsRevision]);

  useEffect(() => {
    const unsubscribe = subscribeApps(() => setAppsRevision((current) => current + 1));
    return unsubscribe;
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    const registration = startPluginContributionSync();
    return () => registration.dispose();
  }, [isTauri]);

  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);

  useEffect(() => {
    activeAppIdRef.current = activeAppId;
  }, [activeAppId]);

  const resetSearchState = useCallback(() => {
    setQuery("");
    setRecentExpanded(false);
    setPinnedExpanded(false);
    setSearchExpanded(false);
    setSelectedKey(null);
    pendingRef.current = null;
    setPendingKey(null);
    setError(null);
  }, []);

  const resetPaletteState = useCallback(() => {
    resetSearchState();
    setMode("search");
    setActiveAppId(null);
    setActiveAppParams({});
    setOpenCreateSnippet(false);
    setInitialTranslateText(undefined);
  }, [resetSearchState]);

  const hideAndResetPalette = useCallback(async () => {
    clearPaletteSession();
    await hidePalette();
    resetPaletteState();
    // Search layout ResizeObserver updates size while the window is still hidden.
  }, [resetPaletteState]);

  /** Blur / outside click: keep opted-in app session for next open. */
  const hidePreservingSession = useCallback(async () => {
    const appId = activeAppIdRef.current;
    if (modeRef.current === "app" && canPersistAppSession(appId) && appId) {
      writePaletteSession(appId);
      await hidePalette();
      return;
    }
    await hideAndResetPalette();
  }, [hideAndResetPalette]);

  const backToSearch = useCallback(() => {
    clearPaletteSession();
    setMode("search");
    setActiveAppId(null);
    setActiveAppParams({});
    setOpenCreateSnippet(false);
    setInitialTranslateText(undefined);
    setError(null);
    setSelectedKey(null);
    if (isTauri) {
      // Defer size until search DOM is laid out so we jump once to measured
      // height instead of 370 -> ResizeObserver correction (height jitter).
      needsSearchSizeRef.current = true;
      void api.getLauncherUsage().then(setUsageItems).catch(console.error);
    }
    window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, [isTauri]);

  const openBuiltinApp = useCallback(
    (appId: string, options?: OpenBuiltinAppOptions) => {
      const app = getBuiltinApp(appId);
      if (!app) return;
      const params = resolveOpenAppParams(options);
      setMode("app");
      setActiveAppId(appId);
      setActiveAppParams(params);
      setOpenCreateSnippet(Boolean(params.createSnippet));
      const translateText =
        typeof params.initialTranslateText === "string"
          ? params.initialTranslateText.trim()
          : undefined;
      setInitialTranslateText(translateText || undefined);
      setQuery("");
      setError(null);
      if (app.persistSession) {
        writePaletteSession(appId);
      } else {
        clearPaletteSession();
      }
      if (isTauri) needsSearchSizeRef.current = true;
      if (isTauri && !options?.restore) {
        // Always record via Rust (local RFC3339) then refresh — JS toISOString() is UTC
        // and lexicographic string sort put builtins after local +08:00 OS app timestamps.
        const usageId =
          app.source === "plugin" ? `plugin:${app.id}` : builtinUsageId(appId);
        void api
          .recordLauncherUsage(usageId)
          .then(() => api.getLauncherUsage())
          .then(setUsageItems)
          .catch((recordError) => {
            console.error(recordError);
          });
      }
    },
    [isTauri]
  );

  useEffect(() => {
    if (!openCreateSnippet) return;
    const timer = window.setTimeout(() => setOpenCreateSnippet(false), 100);
    return () => window.clearTimeout(timer);
  }, [openCreateSnippet, activeAppId]);

  const loadApps = useCallback(
    async (refresh = false) => {
      if (!isTauri) {
        setApps([]);
        setUsageItems([]);
        setLoading(false);
        return;
      }
      try {
        const [nextApps, nextUsage] = await Promise.all([
          refresh ? api.refreshLauncherApps() : api.getLauncherApps(),
          api.getLauncherUsage(),
        ]);
        setApps(nextApps);
        setUsageItems(nextUsage);
        setLoading(nextApps.length === 0);
        setError(null);
      } catch (loadError) {
        setError(errorMessage(loadError, "无法读取本机应用"));
        setLoading(false);
      }
    },
    [isTauri]
  );

  useEffect(() => {
    pendingRef.current = pendingKey;
  }, [pendingKey]);

  useEffect(() => {
    const root = document.documentElement;
    const platformClass = isMacTarget
      ? "command-palette-window--macos"
      : isWindowsTarget
        ? "command-palette-window--windows"
        : "command-palette-window--other";
    root.classList.add("command-palette-window", platformClass);
    document.body.classList.add("command-palette-window", platformClass);
    if (isTauri) void applyThemeFromSettings();
    else applyTheme("system");
    const unsubscribeTheme = isTauri ? subscribeThemeChanges(applyTheme) : () => {};
    void loadApps();

    return () => {
      root.classList.remove("command-palette-window", platformClass);
      document.body.classList.remove("command-palette-window", platformClass);
      unsubscribeTheme();
    };
  }, [isTauri, loadApps]);

  useEffect(() => {
    if (!isTauri) return;
    api
      .getSettings()
      .then((settings) => {
        if (!settings.onboarding_completed) setShowOnboarding(true);
      })
      .catch(console.error);
  }, [isTauri]);

  useEffect(() => {
    if (!isTauri) return;

    const unlistenReminder = listen<ReminderEvent>("reminder", (e) => {
      if (e.payload.type === "eye_care") {
        void openEyeCareReminderWindow();
        return;
      }

      if (e.payload.type === "pomodoro_phase_end") {
        void api.getSettings().then((s) => {
          if (s.sound_enabled) playNotificationSound();
        });
      }

      if (e.payload.type === "todo_due") {
        void api.getSettings().then((s) => {
          if (s.sound_enabled) playNotificationSound();
        });
        const leadText =
          e.payload.lead === "1d"
            ? "将在 1 天后截止"
            : e.payload.lead === "1h"
              ? "将在 1 小时后截止"
              : "已到截止时间";
        void notifyUser("待办提醒", `「${e.payload.title}」${leadText}`);
      }

      setReminder(e.payload);
    });

    const unlistenToast = listen<{ message: string }>("toast", (e) => {
      toast.info(e.payload.message);
    });

    const unlistenCreate = listen("snippets:create-request", () => {
      openBuiltinApp("snippets", { createSnippet: true });
    });
    const unlistenManage = listen("snippets:manage-request", () => {
      openBuiltinApp("snippets");
    });
    const unlistenOpenApp = listen<OpenAppPayload>("command-palette:open-app", (e) => {
      openBuiltinApp(e.payload.appId, { createSnippet: e.payload.createSnippet });
    });

    return () => {
      void unlistenReminder.then((fn) => fn());
      void unlistenToast.then((fn) => fn());
      void unlistenCreate.then((fn) => fn());
      void unlistenManage.then((fn) => fn());
      void unlistenOpenApp.then((fn) => fn());
    };
  }, [isTauri, openBuiltinApp]);

  useEffect(() => {
    if (!isTauri) {
      inputRef.current?.focus();
      return;
    }

    let armed = false;
    let armTimer = 0;
    let unlistenBlur: (() => void) | undefined;

    const restoreSessionIfNeeded = () => {
      if (modeRef.current === "app" && activeAppIdRef.current) {
        // Keep current size — resizing here after show causes a visible flash.
        return true;
      }
      const session = resolveRestorablePaletteSession();
      if (!session) return false;
      openBuiltinApp(session.appId, { restore: true });
      return true;
    };

    const prepareForOpen = () => {
      armed = false;
      window.clearTimeout(armTimer);
      setOpenRevision((current) => current + 1);
      const restored = restoreSessionIfNeeded();
      if (!restored && modeRef.current === "search") {
        // Drop stale keyboard selection (e.g. last opened 内置应用) so current
        // starts at the first 最近使用 item again.
        setSelectedKey(null);
        const focusSearch = () => {
          inputRef.current?.focus();
          inputRef.current?.select();
        };
        window.requestAnimationFrame(focusSearch);
        // Panel may become key a tick after the open event; retry so typing works immediately.
        window.setTimeout(focusSearch, 50);
      }
      armTimer = window.setTimeout(() => {
        armed = true;
      }, 220);
    };

    const unlistenOpen = listen("command-palette:open", prepareForOpen);
    const unlistenShortcutHide = listen("command-palette:shortcut-hide", () => {
      const appId = activeAppIdRef.current;
      if (modeRef.current === "app" && canPersistAppSession(appId) && appId) {
        writePaletteSession(appId);
        return;
      }
      clearPaletteSession();
      resetPaletteState();
    });
    const unlistenIndex = listen("launcher:index-ready", () => void loadApps());
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        // Native file sheets steal focus; suppress blur→hide while they are open (ZTools pattern).
        if (!focused && armed && !pendingRef.current && !isBlurHideSuppressed()) {
          void hidePreservingSession();
          return;
        }
        if (focused && modeRef.current === "search") {
          window.requestAnimationFrame(() => {
            inputRef.current?.focus();
            inputRef.current?.select();
          });
        }
      })
      .then((unlisten) => {
        unlistenBlur = unlisten;
      });

    prepareForOpen();
    return () => {
      window.clearTimeout(armTimer);
      void unlistenOpen.then((unlisten) => unlisten());
      void unlistenShortcutHide.then((unlisten) => unlisten());
      void unlistenIndex.then((unlisten) => unlisten());
      unlistenBlur?.();
    };
  }, [hidePreservingSession, isTauri, loadApps, openBuiltinApp, resetPaletteState]);

  // Keep the controlled input snappy; defer the heavy search/list work.
  const deferredQuery = useDeferredValue(query);
  const liveNormalizedQuery = query.trim();
  const normalizedQuery = deferredQuery.trim();
  const matchedOsApps = useMemo(() => {
    if (!normalizedQuery) return [];
    return apps
      .map((app) => ({ app, score: launcherSearchScore(app, normalizedQuery) }))
      .filter((entry) => entry.score > 0)
      .sort((left, right) => right.score - left.score || left.app.name.localeCompare(right.app.name))
      .slice(0, MAX_SEARCH_RESULTS)
      .map((entry) => entry.app);
  }, [apps, normalizedQuery]);

  const matchedBuiltinApps = useMemo(() => {
    if (!normalizedQuery) return builtinApps;
    return builtinApps
      .map((app) => ({ app, score: builtinSearchScore(app, normalizedQuery) }))
      .filter((entry) => entry.score > 0)
      .sort((left, right) => right.score - left.score || left.app.name.localeCompare(right.app.name))
      .map((entry) => entry.app);
  }, [builtinApps, normalizedQuery]);

  const recentSource = useMemo<RecentEntry[]>(() => {
    const appsById = new Map(apps.map((app) => [app.id, app]));
    const entries: RecentEntry[] = [];

    for (const usage of usageItems) {
      if (!usage.last_used_at && usage.use_count <= 0) continue;

      if (usage.id.startsWith(BUILTIN_USAGE_PREFIX)) {
        const builtinId = usage.id.slice(BUILTIN_USAGE_PREFIX.length);
        const app = getBuiltinApp(builtinId);
        if (!app) continue;
        entries.push({
          key: `recent:builtin:${app.id}`,
          kind: "builtin",
          app,
          last_used_at: usage.last_used_at ?? null,
          use_count: usage.use_count,
        });
        continue;
      }

      if (usage.id.startsWith(PLUGIN_USAGE_PREFIX)) {
        const runtimeAppId = usage.id.slice(PLUGIN_USAGE_PREFIX.length);
        const app = getBuiltinApp(runtimeAppId);
        if (!app) continue;
        entries.push({
          key: `recent:plugin:${app.id}`,
          kind: "builtin",
          app,
          last_used_at: usage.last_used_at ?? null,
          use_count: usage.use_count,
        });
        continue;
      }

      const app = appsById.get(usage.id);
      if (!app) continue;
      entries.push({
        key: `recent:${app.id}`,
        kind: "app",
        app,
        last_used_at: usage.last_used_at ?? null,
        use_count: usage.use_count,
      });
    }

    if (entries.length > 0) {
      entries.sort(
        (left, right) =>
          usageTimeMs(right.last_used_at) - usageTimeMs(left.last_used_at) ||
          right.use_count - left.use_count
      );
      return entries;
    }

    return apps.map((app) => ({
      key: `recent:${app.id}`,
      kind: "app" as const,
      app,
      last_used_at: app.last_used_at ?? null,
      use_count: app.use_count,
    }));
  }, [apps, usageItems]);
  const pinnedApps = useMemo(() => apps.filter((app) => app.pinned), [apps]);
  const visibleRecentApps = recentExpanded
    ? recentSource
    : recentSource.slice(0, RECENT_COLLAPSED_COUNT);
  const visiblePinnedApps = pinnedExpanded
    ? pinnedApps
    : pinnedApps.slice(0, PINNED_COLLAPSED_COUNT);
  const visibleMatchedApps = searchExpanded
    ? matchedOsApps
    : matchedOsApps.slice(0, SEARCH_COLLAPSED_COUNT);

  const quickActionUsageById = useMemo(() => {
    const map = new Map<
      string,
      { last_used_at?: string | null; use_count: number }
    >();
    for (const usage of usageItems) {
      if (!usage.id.startsWith("action:")) continue;
      map.set(usage.id, {
        last_used_at: usage.last_used_at,
        use_count: usage.use_count,
      });
    }
    return map;
  }, [usageItems]);

  const visibleQuickActions = useMemo(
    () => listVisibleQuickActions(normalizedQuery, quickActionUsageById),
    [normalizedQuery, quickActionUsageById]
  );

  const selectionRows = useMemo<PaletteSelection[][]>(() => {
    if (normalizedQuery) {
      const builtinSelections = matchedBuiltinApps.map((app) => ({
        key: `builtin:${app.id}`,
        kind: "builtin" as const,
        app,
      }));
      const appSelections = visibleMatchedApps.map((app) => ({
        key: `search:${app.id}`,
        kind: "app" as const,
        app,
      }));
      const actionSelections = visibleQuickActions.map((action) => ({
        key: `action:${action.id}`,
        kind: "action" as const,
        action,
      }));
      return [
        ...chunkSelections(builtinSelections),
        ...chunkSelections(appSelections),
        ...chunkSelections(actionSelections),
      ];
    }

    const recentSelections = visibleRecentApps.map((entry) =>
      entry.kind === "builtin"
        ? { key: entry.key, kind: "builtin" as const, app: entry.app }
        : { key: entry.key, kind: "app" as const, app: entry.app }
    );
    const builtinSelections = builtinApps.map((app) => ({
      key: `builtin:${app.id}`,
      kind: "builtin" as const,
      app,
    }));
    const pinnedSelections = visiblePinnedApps.map((app) => ({
      key: `pinned:${app.id}`,
      kind: "app" as const,
      app,
    }));
    return [
      ...chunkSelections(recentSelections),
      ...chunkSelections(builtinSelections),
      ...chunkSelections(pinnedSelections),
    ];
  }, [
    builtinApps,
    matchedBuiltinApps,
    normalizedQuery,
    visibleMatchedApps,
    visiblePinnedApps,
    visibleQuickActions,
    visibleRecentApps,
  ]);

  const selections = useMemo(() => selectionRows.flat(), [selectionRows]);
  const selectedSelection = selections.find((selection) => selection.key === selectedKey);
  const activeApp = activeAppId ? getBuiltinApp(activeAppId) : undefined;

  useEffect(() => {
    setSearchExpanded(false);
  }, [normalizedQuery]);

  useEffect(() => {
    if (mode !== "search") return;
    if (selections.length === 0) {
      setSelectedKey(null);
      return;
    }
    if (!selections.some((selection) => selection.key === selectedKey)) {
      setSelectedKey(selections[0].key);
    }
  }, [mode, selectedKey, selections]);

  // After plugin -> search: apply SEARCH_WIDTH + measured height in one shot before paint.
  useLayoutEffect(() => {
    if (!needsSearchSizeRef.current || !isTauri) return;
    needsSearchSizeRef.current = false;
    const height = contentRef.current
      ? Math.ceil(contentRef.current.scrollHeight)
      : SEARCH_FALLBACK_HEIGHT;
    void api.setCommandPaletteSize(SEARCH_WIDTH, height);
  }, [isTauri, mode, activeAppId]);

  // ResizeObserver alone tracks content height. Avoid depending on query/lists —
  // re-subscribing + native set_size on every keystroke is the main input lag source.
  useEffect(() => {
    const content = contentRef.current;
    if (!content || !isTauri) return;
    let frame = 0;
    let debounceTimer = 0;
    let lastHeight = -1;
    let pendingHeight = -1;
    const flushResize = () => {
      if (pendingHeight < 0) return;
      const nextHeight = pendingHeight;
      pendingHeight = -1;
      void api.setCommandPaletteSize(null, nextHeight);
    };
    const resize = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const nextHeight = Math.ceil(content.scrollHeight);
        if (nextHeight === lastHeight) return;
        lastHeight = nextHeight;
        pendingHeight = nextHeight;
        window.clearTimeout(debounceTimer);
        debounceTimer = window.setTimeout(flushResize, PALETTE_RESIZE_DEBOUNCE_MS);
      });
    };
    const observer = new ResizeObserver(resize);
    observer.observe(content);
    resize();
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(frame);
      window.clearTimeout(debounceTimer);
      // Do not flush pending resize on teardown (avoids racing the next mode's size).
    };
  }, [isTauri, mode, openRevision]);

  const executeSelection = useCallback(
    async (selection: PaletteSelection | undefined) => {
      if (!selection || pendingRef.current) return;
      setError(null);

      if (selection.kind === "builtin") {
        openBuiltinApp(selection.app.id);
        return;
      }

      if (selection.kind === "action") {
        const validationError = selection.action.validate?.(liveNormalizedQuery) ?? null;
        if (validationError) {
          setError(validationError);
          return;
        }
        if (selection.action.requiresQuery !== false && !liveNormalizedQuery) return;

        pendingRef.current = selection.key;
        setPendingKey(selection.key);
        try {
          const usageId = quickActionUsageId(selection.action.id);
          if (isTauri) {
            const now = new Date().toISOString();
            setUsageItems((current) => {
              const existing = current.find((item) => item.id === usageId);
              const nextItem = {
                id: usageId,
                pinned: existing?.pinned ?? false,
                last_used_at: now,
                use_count: (existing?.use_count ?? 0) + 1,
              };
              return [nextItem, ...current.filter((item) => item.id !== usageId)];
            });
            void api
              .recordLauncherUsage(usageId)
              .then(() => api.getLauncherUsage())
              .then(setUsageItems)
              .catch(console.error);
          }
          await selection.action.run({
            query: liveNormalizedQuery,
            openApp: openBuiltinApp,
            hideAndReset: hideAndResetPalette,
          });
        } catch (executeError) {
          setError(errorMessage(executeError, "操作没有完成"));
        } finally {
          pendingRef.current = null;
          setPendingKey(null);
        }
        return;
      }

      pendingRef.current = selection.key;
      setPendingKey(selection.key);
      try {
        if (!isTauri) return;
        await api.launchIndexedApp(selection.app.id);
        await hideAndResetPalette();
        void loadApps();
      } catch (executeError) {
        setError(errorMessage(executeError, "操作没有完成"));
      } finally {
        pendingRef.current = null;
        setPendingKey(null);
      }
    },
    [hideAndResetPalette, isTauri, liveNormalizedQuery, loadApps, openBuiltinApp]
  );

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void hideAndResetPalette();
      return;
    }
    if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key)) {
      event.preventDefault();
      const next = moveGridSelection(selectionRows, selectedKey, event.key);
      if (next) setSelectedKey(next.key);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void executeSelection(selectedSelection);
    }
  };

  useEffect(() => {
    if (mode !== "app") return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      backToSearch();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [backToSearch, mode]);

  const togglePinned = async (app: LauncherApp) => {
    if (pendingKey) return;
    const nextPinned = !app.pinned;
    setApps((current) =>
      current.map((item) => (item.id === app.id ? { ...item, pinned: nextPinned } : item))
    );
    if (!isTauri) return;
    try {
      await api.setLauncherAppPinned(app.id, nextPinned);
    } catch (pinError) {
      setApps((current) =>
        current.map((item) => (item.id === app.id ? { ...item, pinned: app.pinned } : item))
      );
      setError(errorMessage(pinError, "无法更新固定状态"));
    }
  };

  const keepSearchFocused = () => {
    inputRef.current?.focus({ preventScroll: true });
  };

  const navigationValue = useMemo(
    () => ({
      openApp: openBuiltinApp,
      backToSearch,
    }),
    [backToSearch, openBuiltinApp]
  );

  const appBodyHeight = activeApp?.defaultSize?.height ?? DEFAULT_APP_HEIGHT;
  const showApp = Boolean(mode === "app" && activeApp);
  const flushApp = Boolean(activeApp && FLUSH_APP_IDS.has(activeApp.id));
  const fillAppHeight = Boolean(
    activeApp && (activeApp.ui.type !== "react" || FILL_HEIGHT_APP_IDS.has(activeApp.id))
  );

  const activeAppNode =
    showApp && activeApp ? (
      activeApp.ui.type === "react" ? (
        <activeApp.ui.component
          onBack={backToSearch}
          openCreateOnMount={activeApp.id === "snippets" ? openCreateSnippet : undefined}
          initialTranslateText={
            activeApp.id === "translate" ? initialTranslateText : undefined
          }
        />
      ) : (
        <PluginAppHost
          pluginId={activeApp.pluginId}
          appId={activeApp.ui.localAppId}
          params={activeAppParams}
          persistSession={activeApp.persistSession}
        />
      )
    ) : null;

  return (
    <BuiltinAppNavigationProvider value={navigationValue}>
      <main className={cn("command-palette-page", showApp && "command-palette-page--app")}>
        <div
          ref={contentRef}
          className="command-palette-panel"
          onMouseDownCapture={(event) => {
            if (!showApp) {
              if (event.target === inputRef.current) return;
              event.preventDefault();
              keepSearchFocused();
            }
          }}
        >
          <header className="command-palette-search">
            {showApp && activeApp ? (
              <>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-lg"
                  className="command-palette-back-button"
                  aria-label="返回搜索"
                  title="返回搜索 (Esc)"
                  onClick={backToSearch}
                >
                  <ArrowLeft />
                </Button>
                <div className="command-palette-app-bar-title">{activeApp.name}</div>
                <div className="command-palette-search-spacer" aria-hidden="true" />
                <div className="command-palette-app-bar-icon" aria-hidden="true">
                  <AppIconView icon={activeApp.icon} className="command-palette-app-bar-icon-glyph" />
                </div>
              </>
            ) : (
              <>
                <input
                  ref={inputRef}
                  value={query}
                  className="command-palette-input"
                  placeholder="搜索应用或输入命令"
                  autoComplete="off"
                  spellCheck={false}
                  aria-label="搜索应用或输入命令"
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setError(null);
                  }}
                  onKeyDown={handleKeyDown}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-lg"
                  className="command-palette-logo-button"
                  aria-label="打开设置"
                  title="打开设置"
                  onClick={() => openBuiltinApp("settings")}
                >
                  <img src="/favicon.png" alt="" className="command-palette-logo" />
                </Button>
              </>
            )}
          </header>

          <div className="command-palette-content">
            {showApp && activeAppNode ? (
              <div
                className={cn(
                  "command-palette-app-host",
                  fillAppHeight && !flushApp && "command-palette-app-host--padded"
                )}
                style={{ height: appBodyHeight }}
              >
                {fillAppHeight ? (
                  activeAppNode
                ) : (
                  <ScrollArea className="h-full">
                    <div className="box-border p-4 px-5 pb-5">{activeAppNode}</div>
                  </ScrollArea>
                )}
              </div>
            ) : loading ? (
              <LauncherLoading />
            ) : normalizedQuery ? (
              <SearchResults
                builtinApps={matchedBuiltinApps}
                apps={visibleMatchedApps}
                totalAppCount={matchedOsApps.length}
                quickActions={visibleQuickActions}
                expanded={searchExpanded}
                query={normalizedQuery}
                selectedKey={selectedKey}
                pendingKey={pendingKey}
                onToggleExpanded={() => setSearchExpanded((current) => !current)}
                onExecute={(selection) => void executeSelection(selection)}
                onTogglePinned={(app) => void togglePinned(app)}
              />
            ) : (
              <DefaultApps
                builtinApps={builtinApps}
                recentApps={visibleRecentApps}
                recentTotal={recentSource.length}
                pinnedApps={visiblePinnedApps}
                pinnedTotal={pinnedApps.length}
                recentExpanded={recentExpanded}
                pinnedExpanded={pinnedExpanded}
                selectedKey={selectedKey}
                pendingKey={pendingKey}
                onToggleRecent={() => setRecentExpanded((current) => !current)}
                onTogglePinnedSection={() => setPinnedExpanded((current) => !current)}
                onExecute={(selection) => void executeSelection(selection)}
                onTogglePinned={(app) => void togglePinned(app)}
              />
            )}
            {!showApp && error ? (
              <p className="command-palette-error" role="alert">
                {error}
              </p>
            ) : null}
          </div>
        </div>
      </main>
      <OnboardingDialog
        open={showOnboarding}
        onComplete={async () => {
          await api.completeOnboarding();
          setShowOnboarding(false);
        }}
      />
      <ReminderDialog event={reminder} onDismiss={() => setReminder(null)} />
      <Toaster position="top-center" richColors toastOptions={appToastOptions} />
    </BuiltinAppNavigationProvider>
  );
}

function DefaultApps({
  builtinApps,
  recentApps,
  recentTotal,
  pinnedApps,
  pinnedTotal,
  recentExpanded,
  pinnedExpanded,
  selectedKey,
  pendingKey,
  onToggleRecent,
  onTogglePinnedSection,
  onExecute,
  onTogglePinned,
}: {
  builtinApps: BuiltinApp[];
  recentApps: RecentEntry[];
  recentTotal: number;
  pinnedApps: LauncherApp[];
  pinnedTotal: number;
  recentExpanded: boolean;
  pinnedExpanded: boolean;
  selectedKey: string | null;
  pendingKey: string | null;
  onToggleRecent: () => void;
  onTogglePinnedSection: () => void;
  onExecute: (selection: PaletteSelection) => void;
  onTogglePinned: (app: LauncherApp) => void;
}) {
  return (
    <div className="command-palette-sections">
      {recentTotal > 0 ? (
        <LauncherSection
          id="launcher-recent-title"
          title="最近使用"
          total={recentTotal}
          collapsedCount={RECENT_COLLAPSED_COUNT}
          expanded={recentExpanded}
          onToggle={onToggleRecent}
        >
          <div className="command-palette-app-grid">
            {recentApps.map((entry) => {
              if (entry.kind === "builtin") {
                return (
                  <BuiltinTile
                    key={entry.key}
                    selectionKey={entry.key}
                    app={entry.app}
                    selected={selectedKey === entry.key}
                    onExecute={() =>
                      onExecute({ key: entry.key, kind: "builtin", app: entry.app })
                    }
                  />
                );
              }
              return (
                <AppTile
                  key={entry.key}
                  selectionKey={entry.key}
                  app={entry.app}
                  selected={selectedKey === entry.key}
                  pending={pendingKey === entry.key}
                  onExecute={() => onExecute({ key: entry.key, kind: "app", app: entry.app })}
                  onTogglePinned={() => onTogglePinned(entry.app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}

      <LauncherSection id="launcher-builtin-title" title="应用">
        <div className="command-palette-app-grid">
          {builtinApps.map((app) => {
            const key = `builtin:${app.id}`;
            return (
              <BuiltinTile
                key={key}
                selectionKey={key}
                app={app}
                selected={selectedKey === key}
                onExecute={() => onExecute({ key, kind: "builtin", app })}
              />
            );
          })}
        </div>
      </LauncherSection>

      {pinnedTotal > 0 ? (
        <LauncherSection
          id="launcher-pinned-title"
          title="已固定"
          total={pinnedTotal}
          collapsedCount={PINNED_COLLAPSED_COUNT}
          expanded={pinnedExpanded}
          onToggle={onTogglePinnedSection}
        >
          <div className="command-palette-app-grid">
            {pinnedApps.map((app) => {
              const key = `pinned:${app.id}`;
              return (
                <AppTile
                  key={key}
                  selectionKey={key}
                  app={app}
                  selected={selectedKey === key}
                  pending={pendingKey === key}
                  onExecute={() => onExecute({ key, kind: "app", app })}
                  onTogglePinned={() => onTogglePinned(app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}
    </div>
  );
}

function SearchResults({
  builtinApps,
  apps,
  totalAppCount,
  quickActions,
  expanded,
  query,
  selectedKey,
  pendingKey,
  onToggleExpanded,
  onExecute,
  onTogglePinned,
}: {
  builtinApps: BuiltinApp[];
  apps: LauncherApp[];
  totalAppCount: number;
  quickActions: QuickAction[];
  expanded: boolean;
  query: string;
  selectedKey: string | null;
  pendingKey: string | null;
  onToggleExpanded: () => void;
  onExecute: (selection: PaletteSelection) => void;
  onTogglePinned: (app: LauncherApp) => void;
}) {
  return (
    <div className="command-palette-sections">
      {builtinApps.length > 0 ? (
        <LauncherSection id="launcher-builtin-results" title="应用">
          <div className="command-palette-app-grid">
            {builtinApps.map((app) => {
              const key = `builtin:${app.id}`;
              return (
                <BuiltinTile
                  key={key}
                  selectionKey={key}
                  app={app}
                  selected={selectedKey === key}
                  onExecute={() => onExecute({ key, kind: "builtin", app })}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}

      {totalAppCount > 0 ? (
        <LauncherSection
          id="launcher-app-results"
          title="本机应用"
          total={totalAppCount}
          collapsedCount={SEARCH_COLLAPSED_COUNT}
          expanded={expanded}
          onToggle={onToggleExpanded}
        >
          <div className="command-palette-app-grid">
            {apps.map((app) => {
              const key = `search:${app.id}`;
              return (
                <AppTile
                  key={key}
                  selectionKey={key}
                  app={app}
                  selected={selectedKey === key}
                  pending={pendingKey === key}
                  onExecute={() => onExecute({ key, kind: "app", app })}
                  onTogglePinned={() => onTogglePinned(app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}

      {quickActions.length > 0 ? (
        <LauncherSection id="launcher-action-results" title="快捷操作">
          <div className="command-palette-app-grid">
            {quickActions.map((action) => {
              const key = `action:${action.id}`;
              const validationError = action.validate?.(query) ?? null;
              const pending = pendingKey === key;
              return (
                <button
                  key={key}
                  type="button"
                  className="command-palette-action-tile"
                  data-selected={selectedKey === key || undefined}
                  disabled={Boolean(validationError) || pending}
                  title={validationError ?? action.title?.(query) ?? action.name}
                  onClick={() => onExecute({ key, kind: "action", action })}
                >
                  <span className="command-palette-action-icon">
                    {pending ? (
                      <LoaderCircle
                        className="command-palette-inline-loader"
                        aria-hidden="true"
                      />
                    ) : (
                      <AppIconView icon={action.icon} />
                    )}
                  </span>
                  <span>{validationError ? "内容过长" : action.name}</span>
                </button>
              );
            })}
          </div>
        </LauncherSection>
      ) : null}
    </div>
  );
}

function LauncherSection({
  id,
  title,
  total,
  collapsedCount,
  expanded,
  onToggle,
  children,
}: {
  id: string;
  title: string;
  total?: number;
  collapsedCount?: number;
  expanded?: boolean;
  onToggle?: () => void;
  children: React.ReactNode;
}) {
  const expandable = Boolean(total && collapsedCount && total > collapsedCount && onToggle);
  return (
    <section className="command-palette-section" aria-labelledby={id}>
      <div className="command-palette-section-heading">
        <h2 id={id}>{title}</h2>
        {expandable ? (
          <button type="button" className="command-palette-expand-button" onClick={onToggle}>
            {expanded ? "收起" : `展开 (${total})`}
          </button>
        ) : null}
      </div>
      {children}
    </section>
  );
}

function BuiltinTile({
  selectionKey,
  app,
  selected,
  onExecute,
}: {
  selectionKey: string;
  app: BuiltinApp;
  selected: boolean;
  onExecute: () => void;
}) {
  return (
    <div
      className="command-palette-app-tile-wrap"
      data-selected={selected || undefined}
      data-selection-key={selectionKey}
    >
      {app.source === "plugin" ? (
        <span className="command-palette-plugin-badge" title="插件">
          插件
        </span>
      ) : null}
      <button
        type="button"
        className="command-palette-app-tile"
        title={app.name}
        onClick={onExecute}
      >
        <span className="command-palette-builtin-icon">
          <AppIconView icon={app.icon} />
        </span>
        <span>{app.name}</span>
      </button>
    </div>
  );
}

function AppTile({
  selectionKey,
  app,
  selected,
  pending,
  onExecute,
  onTogglePinned,
}: {
  selectionKey: string;
  app: LauncherApp;
  selected: boolean;
  pending: boolean;
  onExecute: () => void;
  onTogglePinned: () => void;
}) {
  return (
    <div
      className="command-palette-app-tile-wrap"
      data-selected={selected || undefined}
      data-selection-key={selectionKey}
    >
      <button
        type="button"
        className="command-palette-app-tile"
        disabled={pending}
        title={app.name}
        onClick={onExecute}
      >
        {pending ? (
          <LoaderCircle className="command-palette-app-loader" aria-hidden="true" />
        ) : (
          <AppIcon
            name={app.name}
            iconDataUrl={app.icon_data_url}
            className="size-8"
            fallback="application"
            fallbackClassName="bg-muted text-muted-foreground"
          />
        )}
        <span>{app.name}</span>
      </button>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        className="command-palette-pin-button"
        aria-label={app.pinned ? `取消固定 ${app.name}` : `固定 ${app.name}`}
        title={app.pinned ? "取消固定" : "固定"}
        onClick={onTogglePinned}
      >
        {app.pinned ? <PinOff /> : <Pin />}
      </Button>
    </div>
  );
}

function LauncherLoading() {
  return (
    <div className="command-palette-loading" aria-label="正在读取本机应用">
      <div className="command-palette-section-heading">
        <Skeleton className="h-4 w-16" />
      </div>
      <div className="command-palette-app-grid">
        {Array.from({ length: GRID_COLUMNS }, (_, index) => (
          <div key={index} className="command-palette-loading-tile">
            <Skeleton className="size-8" />
            <Skeleton className="h-3 w-12" />
          </div>
        ))}
      </div>
    </div>
  );
}

function chunkSelections(selections: PaletteSelection[]): PaletteSelection[][] {
  const rows: PaletteSelection[][] = [];
  for (let index = 0; index < selections.length; index += GRID_COLUMNS) {
    rows.push(selections.slice(index, index + GRID_COLUMNS));
  }
  return rows;
}

function moveGridSelection(
  rows: PaletteSelection[][],
  selectedKey: string | null,
  direction: string
): PaletteSelection | undefined {
  if (rows.length === 0) return undefined;

  let rowIndex = 0;
  let columnIndex = 0;
  for (let index = 0; index < rows.length; index += 1) {
    const match = rows[index].findIndex((selection) => selection.key === selectedKey);
    if (match >= 0) {
      rowIndex = index;
      columnIndex = match;
      break;
    }
  }

  if (direction === "ArrowDown" || direction === "ArrowUp") {
    const delta = direction === "ArrowDown" ? 1 : -1;
    const nextRowIndex = rowIndex + delta;
    if (nextRowIndex >= 0 && nextRowIndex < rows.length) {
      rowIndex = nextRowIndex;
      columnIndex = Math.min(columnIndex, rows[rowIndex].length - 1);
    }
  } else if (direction === "ArrowRight") {
    columnIndex = Math.min(columnIndex + 1, rows[rowIndex].length - 1);
  } else if (direction === "ArrowLeft") {
    columnIndex = Math.max(columnIndex - 1, 0);
  }

  return rows[rowIndex][columnIndex];
}

function launcherSearchScore(app: LauncherApp, rawQuery: string): number {
  const query = rawQuery.trim().toLowerCase();
  if (!query) return 0;
  const name = app.name.toLowerCase();
  const haystacks = [name, ...app.keywords.map((keyword) => keyword.toLowerCase())];
  let score = 0;

  for (const value of haystacks) {
    if (value === query) score = Math.max(score, 1000);
    else if (value.startsWith(query)) score = Math.max(score, 820);
    else if (value.split(/\s+/).some((part) => part.startsWith(query))) score = Math.max(score, 680);
    else if (value.includes(query)) score = Math.max(score, 520);
    else if (isSubsequence(query, value)) score = Math.max(score, 260);
  }

  if (score === 0) return 0;
  return score + Math.min(app.use_count, 20) + (app.pinned ? 8 : 0);
}

function builtinSearchScore(app: BuiltinApp, rawQuery: string): number {
  const query = rawQuery.trim().toLowerCase();
  if (!query) return 0;
  const name = app.name.toLowerCase();
  const haystacks = [name, ...app.keywords.map((keyword) => keyword.toLowerCase())];
  let score = 0;

  for (const value of haystacks) {
    if (value === query) score = Math.max(score, 1100);
    else if (value.startsWith(query)) score = Math.max(score, 900);
    else if (value.includes(query)) score = Math.max(score, 640);
    else if (isSubsequence(query, value)) score = Math.max(score, 300);
  }

  return score;
}

function isSubsequence(query: string, value: string): boolean {
  let queryIndex = 0;
  for (const character of value) {
    if (character === query[queryIndex]) queryIndex += 1;
    if (queryIndex === query.length) return true;
  }
  return false;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function applyThemeFromSettings() {
  try {
    const settings = await api.getSettings();
    applyTheme(settings.theme);
  } catch {
    applyTheme("system");
  }
}

async function hidePalette() {
  if (!isTauriRuntime()) return;
  await getCurrentWindow().hide();
}

async function openEyeCareReminderWindow() {
  try {
    await invoke("show_eye_care_overlay");
  } catch (error) {
    console.error("Failed to open eye-care overlay", error);
  }
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
